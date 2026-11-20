# Plan: milestone 4, the loop and a release

Draft 5, 2026-11-19, written from `main` at 0c75eedd (the merge of #244, which closed
milestone 3; its note is `docs/plans/m3-note.md`) and brought to `main` at 0a04cc3c (the
merge of #246, the record of the gate's fourth run), after four outside review rounds
("Review rounds", at the end). Approved: see the last line. Milestone 4 is `SPEC.md` section 16, row 4, "The loop and a release": "Remove
with a check and bounded retries runs as nested calls on one budget; the `prerelease`
workflow passes and a release archive installs and runs on both targets". Beside the row it
takes every carry-over row that names milestone 4 (the table "Carry-over rows owned by
milestone 4, and who takes each", below), among them the one that was milestone E's until
#246 and that the loop cannot wait for: details first as `check`'s default. It ends with
the note of milestone 4.

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
- **A finding** is one answer of the judge's submission that counts against a pass, as
  `check` hands it to its caller. The judge answers in two forms, and both are findings: an
  item of its checklist, which is about one obligation, and a separate observation, which
  cites one or more. A finding says every obligation it cites, how it was answered (its
  **disposition**: `violation`, `held`, `allowed_change`, `hidden`, `cant_tell` or
  `observed`; and its **confidence**: `confident` or `tentative`), and the place the judge
  cited, if it cited one. **An obligation** is a rule of the use case (such as `SF1`) or a
  requirement of the job (`RQ<n>`). **A place** is a box or a polygon on a picture.
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

On 2026-11-20 the orchestrator put three more to the owner, who answered "yes on all but
lets push the optional to the phase E where we beat up each tool individually".

4. **The pre-release checks in two parts** (open question 1): yes. The model tests and
   the live call to each provider are the local leg, on the owner's Mac; GitHub builds and
   installs on both targets. `SPEC.md` section 14's table is edited to say so (R2, item 7,
   made with E1's pull request).
5. **The spec's wording owed since K1 and L1:** yes, as proposed. `SPEC.md` section 3's
   Prepare bullet states a call's plan, and section 7's table of rascal names the loop
   `remove_checked` and the action `carry` (made with E1's pull request; the carry-over
   row on it is closed).
6. **The owner's own pictures move to milestone E.** E1, item 2's optional three to five
   pictures of the owner's, where one pass leaves something behind, are milestone E's,
   where each tool is tested hard on its own. None of E1's 48 runs retried, so the loop's
   retries have no live evidence yet; a carry-over row owned by E says so.

## What the orchestrator decided

Under the rule that review rounds replace the owner's approval of a plan (`SPEC.md` section
14, "Review before building"), the orchestrator settled one of draft 3's open questions on
2026-11-19, and the owner was told the same day.

- **No `unsure` enlarges a mask.** Rascal retried when its checker was unsure and named a
  place. The loop does not: it retries only on a confident report that the named thing is
  still there (L1, item 4), at most three attempts, the first included. This is the
  stricter of the two rules. The live evidence reports how often the loop stopped where
  rascal's rule would have gone on (E1, item 1), so the choice can be looked at again
  with numbers.

## How the work is split

Nine pull requests, where the owner asked for few and large. Sizes are changed lines of
source, tests and hand-written documents, by the rate milestone 3 measured (about 2,500 for
a large pull request); generated reference is counted apart, as in milestone 3.

| Pull request | What | Builds on | Size |
|---|---|---|---|
| A1 | Details first is no longer `check`'s default; and the rows left: the lab's progress and counts, the pairs page. Wave 1's one owner of `docs/carry-over.md` | `main` | Large: about 2,000 (the default 700, the rows 1,300) |
| K1 | What the loop stands on, in the core: a call's plan settled at preparation; a recipe run from a named step; findings as a kind of result; the record's new version; an executor built with a digest alone | `main` | Large: about 2,450 (the plan of a call 750, the runner 450, the kind 150, the record 350, the executor 250, tests 500) |
| K2 | What the loop stands on, in `check`: its findings, in both forms the judge answers in, the same on a cache hit | K1 | Medium: about 1,100 (findings 450, the cache 200, tests 450) |
| L1 | The loop: `remove_checked`, its attempts, its one reservation, which findings may enlarge a mask, cancellation, and what it hands back | A1, K1, K2 | Large: about 2,500 (the action 900, the command and the server 300, the guide 150, `runs stats` 150, tests 1,000) |
| L2 | The located retry: a retry aimed at the places a finding named; a mask carried from a crop to its picture; the masks kept within a bound | L1 | Large: about 2,000 (the carry 450, the retry 700, tests 850) |
| R1 | The archive, its checksums, the bootstrap of what a user must have, the install check, and all of it in CI's `release` job on each runner | `main` | Large: about 2,100 (scripts 600, their tests 700, the licences of what the binary holds 250, the install guide 200, CI 100, the binary's version line 250) |
| R2 | The `prerelease` and `release` workflows, the local leg and its receipt, the rule that refuses a release, and the skill `release` | R1 | Large: about 2,300 (workflows 300, the local leg 450, the receipt and the gate 450, the skill and its listings 300, tests 800) |
| E1 | The evidence before the candidate: the loop run live by a committed script, three cases by hand, the loop's record, the rows left to the orchestrator | all of the above | Medium: about 1,300 (the script and its tally 400, its tests 300, the record and results 600) |
| N1 | After the candidate: the release's record and the note | E1, and a candidate that passed | Small: about 500, no code |

**Waves.** Six, each started from `main` once what it builds on has landed.

1. **A1, K1 and R1** from `main`, side by side.
2. **K2** when K1 has landed; **R2** when R1 has landed.
3. **L1** when A1, K1 and K2 have landed.
4. **L2** when L1 has landed.
5. **E1** when the other seven have landed. The first candidate is tagged on `main` at E1's
   merge.
6. **N1** when a candidate has passed.

K1 was one task of about 2,900 lines in draft 2, over this plan's own size for a split. It
is two now, both before L1: K1 in the core, K2 in `check`, K2 after K1 because it hands
back the kind of result K1 makes.

**What can run side by side, and what cannot.** The loop's strand (A1, K1, K2, L1, L2) and
the release's strand (R1, R2) share no source file but the binary's command list, and run
side by side from start to end. Inside the loop's strand the order is fixed: everything L1
reads (the plan of a call, a run from a named step, findings, the record's version) is K1's
and K2's and lands first, so no later task changes a contract another is building on. A1
runs beside K1 because they share no source file: A1 is in squint's recipe files, the
commands and the lab; K1 is in the core. The two documents every task would otherwise
touch have one owner in a wave too (next paragraph).

**Shared files, each with one owner in a wave.** `docs/carry-over.md`: in wave 1, A1
alone. K1 and R1 do not edit it: R1 closes no row, and the row K1's work answers (an
executor built with a digest alone) is closed in wave 2 by K2, which builds on K1 and is
that wave's one owner of the file (R2 closes no row). L1, L2, E1 and N1 are each alone in
their wave. This plan's own file: a task changes only the lines of its own section, its
box among them. The run record
(`crates/whizbang-core/src/contract/record.rs` and its schema) and the kinds of result
(`contract/descriptor.rs`, `ResultKind`; the description's version): K1 alone. K1 writes the
whole of `run/6`, the fields K2, L1 and L2 fill among them, so the version changes once.
Preparation, admission and the executor (`contract/prepare.rs`, `engine.rs`, where
`Engine::over` is): K1, which is why the row on an executor built with a digest alone is
K1's and not A1's. The recipe runner (`crates/whizbang-core/src/recipe/runner.rs`): K1;
L1 and L2 only call it. `crates/whizbang-squint/src/check.rs`, `cache.rs` and
`use_case/submission.rs`: K2, in wave 2; A1 does not touch them (the default it changes is
in the five recipe files and in the command), and K1 does not either.
`.github/workflows/ci.yml` and the `justfile`: R1 in wave 1, R2 in wave 2. `AGENTS.md`,
`.claude/skills/` and the tests that hold them: R2 alone. The generated reference
(`docs/reference/`) is made again by `just docs` after merging `main`, never merged by hand;
in each wave the second to land merges `main` (a merge commit), runs `just docs` again and
reruns its checks.

**Seams.** Each pull request names where it splits if it passes about 2,500 changed lines,
into a second pull request of its own that lands before anything that builds on it: A1, the
pairs page (its item 9); K1, the executor built with a digest alone (its item 5); K2, none;
L1,
`runs stats` and the server's side (its items 10 and 11); L2, none: the carry is its first
commit and the retry is not split from its bound; R1, the licences of what the binary holds
(its item 3); R2, the skill `release` (its item 6). A split is said in the hand-back and the
owner is told the count. Nothing of the row is dropped to keep the count.

**Review rule.** Every pull request is reviewed whole by a reviewer that did not write it,
as `AGENTS.md` says: the branch and its pull request read against the spec and the task,
`just check` and the range scan run by the reviewer, the reviewed head named, each finding
a blocker or a follow-up. Nothing below narrows that. Beside it, each task names what its
reviewer looks at hardest: A1, that the new default is the arm the gate measured; K1, that
a plan cannot be changed between preparation and admission; K2, that a finding is data,
bounded and held, never instruction, and that no answer that bears on the verdict is left
out; L1, the table of what may enlarge a mask, nested calls under one reservation and what
is sent to a judge; L2, that no retry paints where the table does not allow; R1 and
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
conversation with what they measured (ADR 0029). Its gate rejected it in every run of
2026-11-19, and the run to read is the fourth, the first complete one, every judge
answering: it rejected details first again, helping on none of the three judges
(`docs/results/m3-gates.md`, "The fourth run"). It is still the default of `whizbang squint
check` and of the five use case recipes. The carry-over row was milestone E's, "first
thing", and is milestone 4's since #246, which re-owned it to this task; the reason is
that L1's loop calls squint's checking recipe on every attempt and would otherwise pay
for, and be judged under, a setting its own gate refused. Nothing about details first is removed: it
stays one flag away. With it, in the same pull request, go the carry-over rows that name
milestone 4 and belong to neither strand.

1. **The new default is the arm that was measured, whole.** The gate's control arm (arm
   C, `crates/whizbang-eval/src/matrix.rs`) is three settings together, and the gate
   measured them together: nothing measured first (`details = none`); the judge offered
   `view` and `align`; and a call of `align` answered with its pictures alone (`projection =
   visual_only`). Today's recipes say `first`, offer `view,align,resize` and answer in
   `full`, which is arm F. Turning off the first setting alone would make a fourth
   combination that no gate has measured, with `resize` offered and the measurements of
   `align` shown. So the default becomes arm C exactly, because it is the one arm with
   evidence behind it; `resize` and the full answer come back with `--details first`, as
   arm F had them.
2. **The recipes.** In each of the five files under `crates/whizbang-squint/recipes/`, the
   step `details` says `details = "none"`, and the step `judge` says `tools = "view,align"`
   and `projection = "visual_only"`. The step `details` stays, and a step that measures
   nothing sends nothing and starts no tool. So a recipe called directly, by a caller that
   sets nothing, is arm C.
3. **The command, and the call every door makes.** `whizbang squint check --details`
   defaults to `none`, and `--projection` to `visual_only` with it. `door::check_call`
   builds the recipe's overrides, and today it leaves each one out when it equals what the
   recipe used to say (`if details != "first"`, and the same for the tools, the projection
   and the second look): after item 2 that would make `--details first` do nothing. It is
   changed to state all four settings on every call, whatever the recipes say, so no door
   depends on a recipe's default again. `--details first` then gives arm F as it was
   measured: the details first, `view,align,resize`, and `full`.
4. **The server.** A use case called over MCP that sets nothing is arm C: no details, `view`
   and `align` offered, `visual_only`. **Evaluation is unchanged.** A matrix's arm states
   its own details, tools and projection on `check` itself (`crates/whizbang/src/evaluation.rs`,
   `call_of`), not through a recipe, so every committed matrix and gate means what it meant;
   a test holds an arm F cell and an arm C cell to what they sent before.
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

- [x] Done when:
  `use_case::each_shipped_recipe_called_with_nothing_set_is_the_control_arm`
  (no details, `view,align` offered and no `resize`, `visual_only`),
  `cli::squint_check_with_nothing_set_is_the_control_arm`,
  `cli::squint_check_details_first_measures_first_offers_resize_and_answers_in_full`,
  `door::the_call_of_a_check_states_all_four_settings_whatever_the_recipe_says`,
  `serve::a_use_case_called_with_nothing_set_is_the_control_arm_and_offers_no_resize`,
  `evaluation::an_arm_f_and_an_arm_c_cell_send_what_they_sent_before`,
  `use_case_records::each_use_case_record_holds_two_worked_examples_that_its_recipe_gives`
  (on the records made again),
  `evidence::the_record_of_check_says_details_first_is_off_and_names_its_gate`,
  `lab::each_cell_is_said_as_it_starts_and_as_it_ends`,
  `lab::a_resumed_round_counts_only_what_it_started_and_prints_repeats_apart`,
  `pairs_page::a_pictures_size_on_disk_does_not_tell_its_arm`,
  `pairs_page::a_backspace_takes_back_the_last_pick`, `just docs` regenerated, and the four
  carry-over rows closed.

**Keeping tests lean (A1).** No new picture; each test of the default is a scripted judge on
the pictures the use cases' tests already draw.

**Reworded with the task** (A1, 2026-11-19). Four things the items leave open, as built.

- Item 5 says "`check`'s record". `check` has no record under `docs/reference/tools/`; the
  record read as its own is `docs/results/m2c-use-cases.md`, the record of its use cases'
  live calls, which gains a part, "Details first is off by default".
  `evidence::the_record_of_check_says_details_first_is_off_and_names_its_gate` holds that
  part, the guide to the command (`docs/guides/config.md`) and the five use cases' records
  to the same words. The use cases' table of worked examples gains a column, the tools
  offered to the judge, since under the new default the calls of `details` are none in
  every row.
- Item 3: the two tests named `cli::` go through `door::Measuring::stated`, which the
  command hands `--details` and `--projection` as they were typed or not at all; no test
  can put a scripted judge behind the real binary. And `--detail`, which names the tools
  measured first, is refused without `--details first`
  (`door::DETAIL_WITHOUT_FIRST`; `cli::squint_check_detail_tiles_on_a_pair_is_refused_naming_the_use_case`):
  with nothing measured first by default it would otherwise measure nothing and say
  nothing.
- Item 8: a cell whose request an earlier run answered is counted by how that run ended,
  not as never run. Why the `m3-seam` leg did not recognise its earlier attempts is not
  found; the test stands a recogniser that forgets the leg before in its place.
- Item 9: "a backspace takes back the last pick" is shown by running the page's own script
  under `node` with only what it touches of a page, where a `node` is installed (both
  hosted runners, the owner's Mac); no JavaScript engine is among the pinned tools, so
  elsewhere the test holds the page to its text. The carry-over list has the row. Each
  picture is brought to its pair's size by one chunk of padding in the PNG, not by another
  encoder.

## K1. What the loop stands on, in the core (large)

### Goal

Additions to the core below the loop, none of them a loop, each needed before L1 can be
written as this plan describes it. The first review round found that draft 1 promised
behaviour at preparation that no action can give today, and put the loop before the things
it reads. They are one task so that the record's version, the kinds of result and the rules
of preparation each change once, in one place.

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
   amended; a new description version): one JSON document, at most 32 KiB, whose shape K1
   fixes by a schema and whose contents K2 makes. Each finding holds: which form of answer
   it is; **every** obligation it cites, by id; its disposition and its confidence; and,
   when one was cited, its place: the image it is on and a box or a polygon in fractions of
   that image. No text of a judge's is in it. The executor refuses findings from an action
   that does not declare them, as it does a verdict; a recipe whose last step declares
   findings hands them to its caller with the verdict.
4. **The record's new version, `run/6`** (by the rule of ADR 0005, with its schema and
   fixed vectors, reading `run/5`), written whole here so that it changes once: the plan of
   a call; where a recipe's run started and what it was given; the findings a run handed
   back, as counts by obligation, disposition and confidence, and the places; and a loop's
   attempts, the list L1 and L2 fill: for each attempt its number, the runs it made in
   order, the verdict or the reason there is none, what the retry changed to make it (a
   wider `grow`; the crops cut and the masks they gave), and what its requests settled;
   then which attempt was kept and why the attempts ended. K1 holds the lists by their
   schema and by fake actions that fill them; K2, L1 and L2 add no field.
5. **An executor built with a digest alone** (`Engine::over`, in the file this task owns;
   the carry-over row that names milestone 4) is compiled for tests alone, as
   `OneWorker::starting` is, or backed by one walk of the config, as the row allows. K1
   builds it and does not edit the row: K2 closes it (below).

- [x] Done when:
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
  `executor::findings_from_an_action_that_does_not_declare_them_end_the_run_as_an_error`,
  `executor::findings_that_are_not_of_their_schema_end_the_run_as_an_error`,
  `runner::a_recipe_hands_its_caller_the_findings_of_its_last_step`,
  `describe::the_description_lists_findings_among_an_actions_results_and_which_actions_plan`,
  `record::run_6_holds_a_plan_a_start_findings_and_a_loops_attempts`,
  `record::run_6_reads_a_record_of_run_5`,
  `asked::an_executor_built_with_a_digest_alone_is_a_tests_alone` (or the test of the one
  walk, as the builder chose), `scripts/tests/test-features.sh` and
  `scripts/tests/test-crate-graph.sh` passing with the new feature, and `just docs`
  regenerated. The carry-over row on that executor is closed by K2, in the next wave,
  with a line naming this pull request: A1 alone edits `docs/carry-over.md` in wave 1.

**Keeping tests lean (K1).** Planning is tested on a fake action with fake backends whose
bounds a test states; findings on a fake action that hands back a document a test writes;
the runner's tests use the fake segmenter.

**Split at the seam, with the task.** K1's first pull request builds items 1 to 4 and
passes the size for a split (about 3,700 changed lines of source, tests, schemas and
documents, the generated reference apart: the record's schema and its fixed vectors, and
the tests of a plan through the executor, the command and the server, came to more than
the estimate). So it stops at the seam this plan names ("Seams"): item 5, the executor
built with a digest alone, is a second pull request of K1's own, which lands before K2
closes the carry-over row. The box above stays open until that pull request: its clauses
`asked::an_executor_built_with_a_digest_alone_is_a_tests_alone` (or the test of the one
walk), and `scripts/tests/test-features.sh` and `scripts/tests/test-crate-graph.sh` with
the new feature, are the second pull request's. Every other clause is shown by its test
in the first. Where the tests are: those named `prepare::`, `admit::` and
`executor::an_action_is_handed_the_plan_it_was_prepared_with` are in the core's
`contract::plan`, a module for each; the two of `executor::findings_` in the core's
`contract::findings::executor`; `runner::`, `describe::` and `record::` in the core's
modules of those names; `cli::` and `serve::` in the binary's.

**After the review of the first pull request (K1).** Fixed in it: the findings' reader
refuses what the schema refuses (a finding without its `confidence` or `place`, a null
`box` or `polygon`); a run from a step given more than a record lists is refused; a
repeated request id is told the plan of its first run; a recipe's input may not take
the runner's `from` names; a run from a step may not leave unbound what a whole run
binds; a plan is kept when only its reason is too long after redaction; a panic while
planning refuses the call; and the reader's sentence for an unknown field is whole again.
Item 5 was looked at again and does not fit beside it: either way of building it changes
the tests of every crate that builds an executor (a feature on their manifests and the
crate graph's checks, or a held backend for every roster a test gives), which is more
than a few hundred lines and reaches scripts and tests other tasks of this wave own. It
goes to wave 2, before K2 closes the row.

**Item 5, built in wave 2 with K2's pull request, as its own commit.** The executor
given its backends apart (`Engine::new`, `Engine::over`, `Engine::with_capabilities`) is
compiled for tests alone, behind a feature of the core, `fakes`; every build has
`Engine::reading` and `Engine::writing`, over a source of the config and nothing beside
it, which is how the binary's wiring builds its two executors now (ADR 0015, "One
snapshot of the config"). The feature is turned on in one manifest, the test kit's,
not five: the test kit is a development dependency alone, so every crate's tests build
the core with it and the binary's build never does. So the crate graph has no new edge
and `scripts/tests/test-crate-graph.sh` is unchanged; `scripts/check-features.sh` holds
the binary's build to the core without the feature, and
`scripts/tests/test-features.sh` plants it there and sees the check fail. The test is
`asked::an_executor_built_with_a_digest_alone_is_a_tests_alone` (the core's
`contract::asked`), which also holds that an executor every build has resolves nothing
it does not hold. The one walk at admission, the other way the row allowed, was tried
first: it refused 153 of the core's tests, whose rosters price or resolve a call that
asks no backend for anything.

**Owed after K1** (follow-ups of that review, not built here):

- Squint's five use case recipes now offer a `from` (their step `check`) that can never
  run: `check` takes what `details` measured, and a run from a step refuses that. The
  runner should not offer `from` for a step no stretch can start at.
- The command's door prepares a planned call twice, once to say its plan and once at
  admission; a call that plans nothing is prepared once more than before too.
- `SPEC.md` section 3, "The execution contract", says nothing of a plan in its Prepare
  bullet: an edit of wording for the task that next touches the spec, with ADR 0004.

**Decided with the task (K1).**

- A plan is more than a loop's attempts: it states the requests of each capability, and
  a loop's attempts beside them, so that a recipe run from a step can plan too (item 2:
  "nothing is reserved for them" cannot be said by needs that apply by an argument's
  presence alone). Every recipe of two steps or more is therefore listed as an action
  that plans; it plans only a call that gives `from`.
- A call with a plan has a fingerprint of the kind `prepared/2`; one without is of
  `prepared/1` as before (ADR 0004, "A call's plan").
- A record is written as `run/6` only when it holds one of the four new groups, as
  `run/5` is written only for a recipe's steps (ADR 0033): every other record is
  `run/4` or `run/5`, byte for byte as before. So "reading `run/5`" needs no
  conversion, no store is migrated, and the store's own format does not move.
- A run from a step still gives the recipe's own required inputs, those only a skipped
  step takes among them (ADR 0033, "A run from a step").
- The findings' schema carries the decision of the whole submission, `complete` and the
  counts of what was left out (K2, items 1 and 2), since K2 adds no field.

## K2. What the loop stands on, in `check` (medium)

### Goal

`check` hands its caller its findings, complete, and the same whether or not the answer came
from the decision cache. The second review round found that draft 2's findings were the
checklist's items alone, each with one obligation, while a submission also holds separate
observations that cite several obligations and bear on the verdict
(`crates/whizbang-squint/src/use_case/submission.rs`): a report of damage made that way
would have been lost before the loop classed anything.

1. **Both forms of answer.** `check` declares findings and hands back one for every answer
   of the submission whose row is not "recorded", that is, every answer that makes the
   verdict `fail` or `unsure`: the checklist's items, and the separate observations. Each
   keeps every obligation it cites, its disposition and its confidence, as the submission
   was read. They are made from what `check` already keeps in `judged.json`, by the same
   code, so the two cannot differ. **The document also carries the decision of the whole
   submission**, the verdict as `check` gave it, worked out from every answer before
   anything is cut: so nothing after `check` works a verdict out from a list that may be
   short. Where the findings are complete, the verdict worked out from them alone is that
   decision, and a test holds it; where they are not, only the carried decision is right,
   and a test holds that too.
2. **A finding is data from a model, and a short list says it is short.** A submission may
   hold any number of answers, and every one enters the verdict. At most 32 findings are
   handed back, in the order of the submission; among them at most eight places; a place
   on an image the run was not given is left out; numbers are held inside the picture; no
   text of the judge's travels with a finding. Whenever anything was left out, a finding,
   a place, or a part cut to fit the 32 KiB, the document says **`complete: false`** and
   counts what was left out; otherwise it says `complete: true`. A caller that would act
   on the findings must read that word first, because the finding that was dropped may be
   the one that matters: the loop's table makes it its first row (L1, item 4). The carried
   decision is never cut.
5. **The row K1 answered.** This pull request closes the carry-over row on an executor
   built with a digest alone, with a line naming K1's pull request and its test.
3. **The same on a cache hit.** A run of `check` answered from the decision cache hands
   back the findings the earlier run handed back. They are worked out again from the
   `judged.json` the hit holds, by the same code, and held by the same rules. An entry
   written before this change that kept no `judged.json` is not answered from: the check
   runs, and the new entry replaces it. So two identical checks give a caller identical
   findings whether or not the second was answered from the cache.
4. **The use cases.** Each of the five recipes hands its caller the findings of its step
   `check`; `whizbang squint check` prints their counts; the server's answer carries them.

- [x] Done when:
  `check::a_checklist_item_and_a_separate_observation_are_each_handed_back_as_a_finding`,
  `check::a_finding_keeps_every_obligation_it_cites_its_disposition_and_its_confidence`,
  `check::the_verdict_worked_out_from_complete_findings_is_the_decision_they_carry`,
  `check::findings_carry_the_decision_of_the_whole_submission_whatever_was_cut`
  (32 tentative violations kept and the only confident one cut: the decision carried is
  `fail`, where the kept list alone would give `unsure`; on a check that was made and on
  the same check answered from the cache),
  `check::an_answer_whose_row_is_recorded_is_no_finding`,
  `check::a_place_on_an_image_the_run_was_not_given_is_left_out_and_counted`,
  `check::no_more_than_32_findings_and_eight_places_are_handed_back_and_no_text_of_the_judges`,
  `check::findings_cut_short_in_any_way_say_they_are_not_complete_and_count_what_was_left_out`
  (a 33rd finding, a ninth place, a place on another image), on a check that was made and
  on the same check answered from the cache,
  `check::a_check_answered_from_the_cache_hands_back_the_findings_of_the_run_that_answered`
  (on a submission with a checklist violation of one obligation beside a separate
  observation that cites two),
  `cache::an_entry_that_kept_no_judged_document_is_not_answered_from`,
  `use_case::a_use_case_recipe_hands_its_caller_the_findings_of_its_last_step`,
  `cli::squint_check_prints_the_counts_of_its_findings`, `just docs` regenerated, and the
  carry-over row on an executor built with a digest alone closed.

**Keeping tests lean (K2).** Findings are read from scripted submissions on the pictures the
use cases' tests already draw.

**Where the tests are (K2).** Those named `check::`, `cache::` and `use_case::` are in
squint's tests, in a module of their own, `findings` (`findings::check::…`,
`findings::cache::…`, `findings::use_case::…`,
`crates/whizbang-squint/tests/it/findings.rs`): the files the names would put them in
are being changed by A1 in the same wave. `cli::squint_check_prints_the_counts_of_its_findings`
is the binary's; beside it `serve::check_over_stdio_answers_with_the_counts_of_its_findings`
shows the server's answer of item 4. Each of the decision table's inputs (L1, item 4) is
run on a check that was made and on the same check answered from the cache: a confident
`RQ1` violation alone (in `…the_verdict_worked_out_from_complete_findings…`), beside a
separate `SF1` violation (`…a_checklist_item_and_a_separate_observation…`), 32 tentative
kept with the only confident one cut (`…findings_carry_the_decision…`), and an
observation that cites several obligations (`…a_finding_keeps_every_obligation…`,
`…a_check_answered_from_the_cache…`). The job of those tests is the one a loop gives a
check: `source-fidelity` with `RQ1` to `RQ3`.

**Reworded with the task** (K2, 2026-11-19). One clause of the "Done when" above is met
otherwise than it is worded.

- "`check::findings_cut_short_in_any_way_say_they_are_not_complete_and_count_what_was_left_out`
  (a 33rd finding, a ninth place, a place on another image), on a check that was made
  and on the same check answered from the cache". The 33rd finding and the ninth place
  are shown so. A place on another image cannot come from a check that was made: a
  submission whose location names a frame outside its job is refused before anything is
  kept (`use_case/submission.rs`, `place`), and the job's images are the call's. It is
  shown, there and in `check::a_place_on_an_image_the_run_was_not_given_is_left_out_and_counted`,
  on the document a run keeps of what it decided, changed by the test, which is what
  the findings are made from on a check that was made and on a hit alike
  (`findings::of_judged`). The same test shows the other ways a list is short on that
  document: a box with no area, an obligation's id the kind does not take, and a list
  over 32 KiB.

**After the review of the pull request (K2).** Fixed in it: the two other ways
`cache::find` passes a run over have their tests (a document that gives no findings; one
whose findings would not carry the recorded verdict); a run whose kept document held a
secret keeps no key, so that no entry is left that a hit cannot read; an obligation's
id is held to four digits where a job is resolved; a polygon that encloses no area is
left out and counted, as a box with none is; and an answer that counts and is about no
obligation is counted as a finding left out. Their tests are
`cache::an_entry_whose_judged_document_gives_no_findings_is_not_answered_from`,
`cache::an_entry_whose_findings_would_not_carry_its_verdict_is_not_answered_from`,
`cache::a_check_whose_kept_document_held_a_secret_keeps_no_key`,
`use_case::a_requirements_id_is_held_to_what_a_finding_may_cite`, and two more cases of
`check::findings_cut_short_in_any_way_say_they_are_not_complete_and_count_what_was_left_out`.

**Owed after K2** (a follow-up of that review, not built here):

- The item of a reference image is handed back as one finding for each obligation, and
  each of them carries the item's one location, so one location takes as many of the
  eight places as the item has obligations. One place for the item, shared by its
  findings, would leave the others for other answers. For the task that first reads
  places under a use case with references; the loop's `source-fidelity` has no such item.

**Decided with the task (K2).** ADR 0035, "What `check` hands back", has each.

- **Made from `judged.json` alone,** by one function, on a check that was made (from
  the document it has just written) and on a hit (from the document of the run that
  answered). For that, what is kept of a checklist item names the obligations the item
  is about. `check` is 1.7.0.
- **An old cache entry is never met, and there is no shim.** The version of `check` is
  in the cache's key, so a check of 1.7.0 finds no run of 1.6.0. A run of the same key
  that kept no `judged.json`, or one the findings cannot be made from, or whose findings
  would not carry its verdict's decision, is passed over: the check runs, and the newer
  run is the one found next.
- **The item of a reference image** (`chk:ref:<image>`) is about several obligations,
  and an item of `findings/1` cites one: it is handed back as one finding of the form
  `item` for each obligation, in the rules' order, so none is dropped and no field is
  added. The loop's use case, `source-fidelity`, has no such item.
- **A place is a box or a polygon.** A location that is the whole image, or an absence,
  is no place and is not counted as one left out: nothing can be aimed at it.
- **Under `general`,** where the judge answers with a word and no submission, `check`
  hands back no findings.
- **The doors print nothing for a check with no finding and nothing left out,** so a
  pass reads as it did; otherwise one line says whether the list is complete, with
  what was left out, and one line follows for each count the record keeps.

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
at the places a finding named. Item 4 is the one table that says what may enlarge a mask,
for both.

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
4. **What may enlarge a mask: one table.** A retry paints more of the picture than the
   attempt before it. It is allowed only by a finding in which the judge says, with
   confidence, that the named thing is still there. This table is the whole rule, for the
   wider mask of this task and the located retry of L2. A finding is read by three things
   and nothing else: every obligation it cites, its disposition and its confidence; never
   by the judge's words, and the same for both forms of answer. "Must be preserved" below
   means `SF1` (a protected subject), `SF2` (the scene's geometry) or `RQ2` (nothing else
   changed).

   | Row | A finding that cites | With disposition and confidence | What it does |
   |---|---|---|---|
   | 0 | (not a finding: the findings say `complete: false`, K2, item 2) | | **Ends the attempts.** Findings that were cut short never enlarge a mask: what was dropped may be a row 1 finding. This row is read before any other |
   | 1 | anything that must be preserved, alone or beside other obligations, `RQ1` among them | `violation`, confident or tentative | **Ends the attempts.** The removal damaged something, and painting more cannot mend it. This row beats every other |
   | 2 | `RQ1`, and nothing that must be preserved (`RQ3` or `SF4` may be cited beside it) | `violation`, confident | **Allows a retry**, and only its own place may be used: the located retry when it cites a place on the candidate (L2); a wider mask, once in a run, when it cites none |
   | 3 | `RQ1`, and nothing that must be preserved | `violation`, tentative | Enlarges nothing. The judge is not sure the thing is there |
   | 4 | anything | `held`, `allowed_change`, `hidden` or `cant_tell`, at either confidence; or `observed` | Enlarges nothing. None of these says the named thing is still there, though at a tentative confidence, or where the judge cannot tell, they make the verdict `unsure` |
   | 5 | `RQ3` or `SF4` alone | `violation` | Enlarges nothing, and forbids nothing: beside a finding of row 2 the retry goes on |

   **What the loop does with a verdict's findings.** If the findings are not complete (row
   0), the attempts end; the verdict the loop hands back and ranks its attempts by is
   always the decision the findings carry, never one worked out from the list. If any
   finding is of row 1, the attempts end. Otherwise, if at
   least one is of row 2 and the plan has an attempt left,
   one retry is made, from the places of the row 2 findings alone. Otherwise the attempts
   end. So a `fail` retries only on a confident report that the thing remains, and an
   `unsure` never enlarges a mask: this is narrower than rascal, which retried on an
   `unsure` that named a place ("What the orchestrator decided"). When the attempts end the loop hands
   back its best attempt with the verdict (item 8).
5. **What a retry changes, in L1.** After a verdict the table allows a retry on, whose row
   2 findings cite no place: the shape stretch is run again from the same found mask with
   its step `grow` given twice the `px` (the runner's own override, `set`), once in a run:
   rascal's "a `fail` that names no place widens the mask once". Until L2 lands, a row 2
   finding that cites a place is treated as one that cites none. Nothing else changes
   between attempts: the same phrase, segmenter, filler and judge.
6. **What ends the attempts.** A verdict of `pass`. The last attempt the plan allows. A
   verdict the table allows no retry on. A wider mask already used once. A check that could
   not run. A retry whose shaped mask is the one before it. A retry whose mask could not be
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
     included: rascal's limit, kept.
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

- [x] Done when:
  `checked::a_first_attempt_that_passes_is_one_find_one_shape_one_fill_and_one_check`,
  `checked::no_fill_is_asked_before_its_shaped_mask_is_in_hand`,
  `checked::a_retry_whose_shaped_mask_is_the_one_before_is_not_filled`, and one test for
  each row of the table, each run twice, on a check that was made and on the same check
  answered from the cache, with the same result:
  `table::row_0_an_rq1_violation_kept_with_an_sf1_violation_cut_off_ends_the_attempts`,
  `table::row_0_findings_whose_only_confident_violation_was_cut_off_enlarge_nothing`,
  `table::row_0_findings_cut_short_end_the_attempts_whatever_was_kept`,
  `table::row_1_a_violation_of_a_protected_subject_of_the_geometry_or_of_rq2_ends_the_attempts`
  (each of the three, confident and tentative, as a checklist item and as a separate
  observation),
  `table::row_1_beats_row_2_an_rq1_violation_beside_a_separate_sf1_violation_ends_the_attempts`,
  `table::row_1_a_finding_that_cites_rq1_and_sf1_together_ends_the_attempts`,
  `table::row_2_a_confident_violation_of_rq1_with_no_place_widens_once_and_is_checked_again`,
  `table::row_2_a_second_widening_in_one_run_is_not_made`,
  `table::row_3_a_tentative_violation_of_rq1_enlarges_nothing`,
  `table::row_4_held_allowed_change_hidden_cant_tell_and_observed_each_enlarge_nothing`
  (each disposition, at each confidence it takes, about `RQ1`),
  `table::row_5_a_violation_of_rq3_beside_a_confident_violation_of_rq1_still_retries`,
  `table::row_5_a_violation_of_rq3_alone_ends_the_attempts`,
  `table::a_finding_is_read_by_its_obligations_disposition_and_confidence_in_either_form`,
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

**Split at the seam, with the task** (L1, 2026-11-20). The task passes the size for a
split: about 4,800 changed lines of source, tests and documents, the generated reference
apart, where the estimate was 2,500. The tests are 2,900 of them and were estimated at
1,000: the "Done when" names 45, and a run of the loop takes a fixture of its own. So it
stops at the seam this plan names ("Seams"). **The first pull request** is the loop and
its record: items 1 to 8, the plan and the unchecked pass of item 9, and items 12 and 13,
with the loop listed by `describe` (item 10). It is still about 3,800 lines, 2,500 of them
tests. **The second,** of L1's own and stacked on the first, is the doors: the command and
what it prints (items 9 and 10), the server's answer (item 10), `runs stats` and `runs
show` (item 11), and the guide with its figures (item 7). Its clauses of the "Done when"
are `serve::the_loop_is_offered_as_a_tool_…`, `cli::rascal_remove_checked_prints_…`,
`stats::runs_stats_counts_a_loops_attempts_and_retries` and
`guides::the_loops_guide_states_the_bound_of_an_attempt_…`, and two of the four carry-over
rows ("Say on the terminal when a run was not checked"; "The documented per-call cap for
a check is one cent"). Every other clause is shown by its test in the first.

**Where the tests are (L1).** All are the binary's (`crates/whizbang/tests/it/`), because
only the binary holds rascal's loop and squint's check together: `checked::`, `table::`,
`budget::`, `nested::` and `guides::` are modules of their own, and `record::`, `recipe::`,
`describe::`, `serve::`, `cli::`, `stats::` and `model_tier::` are in the files of those
names. The fixture is `checked::Looped`: the binary's registry and runner, the real ledger
and meter, a config the test writes, and a kind of the test's own for the three backends.
Its store is in memory (`checked::Memory`), made in the test's process. On the real disk
a run of the loop is some four hundred small writes, renames and new folders, and many
tests doing that at once wait on each other in the system: the loop's tests took 88
seconds of processor time together there, and take 12 in memory.

**The box stays open for one clause** (L1, 2026-11-20), after both pull requests.
`model_tier::remove_checked_makes_its_mask_with_sam3_under_a_scripted_judge` is written,
is `#[ignore]`d and is in the model tier's filter, and has not been run: a builder loads
no real model. The box is ticked when `just test-models model_tier` has run it on the
Mac. Every other clause is shown by its test, `just docs` is regenerated, and the four
carry-over rows are closed. **Ticked with task L2** (2026-11-20): the orchestrator ran the
model tier on the Mac at 3b92a258, and all 8 `model_tier` tests passed, that one among them.

**Reworded with the task** (L1, 2026-11-20). Three clauses of the "Done when" above are
met otherwise than they are worded.

- "`table::row_4_held_allowed_change_hidden_cant_tell_and_observed_each_enlarge_nothing`
  (each disposition, at each confidence it takes, about `RQ1`)". `held`, `hidden` and
  `cant_tell` are shown so, through the loop, made and answered from the cache. A
  submission cannot say `allowed_change` or `observed` of `RQ1`: an allowed change names
  an exemption that waives the obligation, and no rule waives a job's requirement; and
  `observed` is refused of an item that must hold (`use_case/submission.rs`). Those two
  are shown on the findings document a check hands back, read by the loop's own table
  (`checked::table::read`), as K2 showed a place on another image.
- "`table::row_2_a_confident_violation_of_rq1_with_no_place_widens_once_and_is_checked_again`".
  A submission's violation always has a location, so "no place" is a violation that
  cites the whole image, which `check` hands back with no place (ADR 0035). The same test
  shows the finding that cites a box, treated as one that cites none until L2.
- "`budget::three_attempts_at_their_largest_stay_inside_a_trees_runs_and_a_runs_calls`".
  The loop of this task makes two attempts at most, since a wider mask is used once: the
  test counts the runs of that run (26: the loop, 14 for the first attempt, 11 for a
  retry) and holds three attempts with every tool call a check may dispatch, 133 runs, to
  the limits by arithmetic on what it counted. L2 raises it with a run of three.

**Reworded with the task** (E1, 2026-11-20, on the orchestrator's word). "one test for
each row of the table, each run twice, on a check that was made and on the same check
answered from the cache, with the same result" is met otherwise for two of the tests.
`table::row_1_a_violation_of_a_protected_subject_of_the_geometry_or_of_rq2_ends_the_attempts`
has twelve cases and `table::row_4_…` six through the loop: every case is still run on a
check that was made, with its assertions, and the first case of each is run a second time
answered from the cache; the others are not. The reason is test time: the hosted Mac read
262.6 seconds of its 300 on the head of L2's second pull request, the second runs of these
two tests were sixteen runs of the loop, and "Owed after L1" named them as the first to
trim. That a check answered from the cache hands the loop the same findings is held for
every other row's test, each run twice, and by K2's tests of the cache.

**Decided with the task (L1).** ADR 0039 has each.

- **The check is called as the recipe ships it:** its job, its two images and the judge's
  name, and no override of a step. A door states all four settings of the judge's arm on
  every call (A1, item 3); the loop is no door, and "does not choose squint's tools"
  (`SPEC.md` section 3). What the recipe ships is the control arm.
- **The short result is the run's measurements** (`remove_checked/1`), and the doors read
  the same from the run's record, so the loop's descriptor declares a verdict, findings
  and measurements.
- **An attempt that was begun is listed,** one that made no fill among them (a retry whose
  mask was the one before, or could not be made or filled): its reason says so, its runs
  are listed, and it is never the one kept. "Attempts made" counts those.
- **Each call of the loop's is labelled in the record** by its attempt and its stretch
  (`a1_find`, `a1_shape`, `a1_fill`, `a1_check`, `a2_shape`, …).
- **Under `--check required` a check the cap does not cover** is planned as one checked
  attempt and refused as over the cap; under `auto` it is one unchecked pass.
- **`whizbang runs show` prints a loop's attempts,** which the plan did not ask and the
  record's reader needs.
- **The command reads the decision cache** where the config turns it on, as `squint check`
  does; the server already did.
- **Live telling.** The command starts the run and looks at what it has said every 250
  milliseconds, since the executor has no way to wait for an event; once the run is over
  it reads the rest from the run's journal, so every attempt is told, in order.
- **The doors' tests.** A door reads the store through the binary's own readers, so the
  command's test and the server's keep their stores on the real disk, written without
  the syncs, two runs of the loop each; the test of `runs stats` runs its loops in
  memory and writes the store's files to its scratch folder once, before the binary
  reads them (`Looped::spill`).
- **After the review:** a look at a running loop that fails no longer ends the command,
  which goes on to wait for the run (`cli::…`, through an executor whose looks fail);
  and under `--check auto` a judge that names a credential is not asked for its env
  file, so that a missing key is one unchecked pass, as item 9 says; only `--check
  required` asks (`cli::…`, through the real binary).

**After the review of the pull requests (L1).** Fixed in them: a deadline that passes
after a fill completed ends the attempts with "the deadline passed after attempt N" and
keeps the best, and a child that ends cancelled without the loop's own signal is a call
that stopped, not a deadline; a cancelled run's record never says an attempt was kept; a
phrase over the segmenter's 512 bytes is refused before any call is made (at the start of
the run: a descriptor states no length of a text, so preparation cannot hold one); the
loop is built only over a recipe whose `grow` comes directly after `select` and whose
`fill` comes directly after `feather`, and last; the plan states the filler's requests as
what one attempt sends times the attempts; and `protect`, a named filler and a named
judge each go through the loop under a test. Their tests are
`checked::a_deadline_that_passes_after_a_fill_completed_ends_the_attempts_and_keeps_the_best`,
`checked::a_phrase_over_the_segmenters_limit_is_refused_before_any_call_is_made`,
`checked::the_loop_is_built_only_over_a_recipe_whose_stretches_are_where_it_runs_them`,
`checked::a_mask_to_protect_is_kept_off_the_mask_of_every_attempt`,
`checked::a_filler_and_a_judge_the_call_names_are_the_ones_asked_and_reserved_for` and
`checked::an_unchecked_pass_is_told_as_it_starts_and_ends_with_its_reason`. The model
tier's `remove_prompt_makes_a_mask_with_sam3_on_the_mac` had failed since the recipe
gained its fill (plan m3, task F1): it ran the recipe whole under a config with no
filler. It now stops after `feather`.

**Notes for L2.**

- The seam is one place: in `checked.rs`, `Looping::run` decides what the next attempt
  changes where it reads `table::Reading::Remains(places)`. The places are already there,
  each in the findings' order, `None` for a finding that cites none; L1 ignores them.
  `Changed::Located` and `Attempt::changed` are K1's and ready.
- The found mask of an attempt is one identity (`found`); a located retry replaces it
  with what `remove_prompt` from `select` until `select` hands back. The loop freezes its
  calls to `remove_prompt` and `source-fidelity`: `view` and `segment` must join that
  list, and the descriptor's needs must then count the segmenter's requests of a retry
  in the plan (`Plan::sending(Capability::Segment, …)`).
- A place's `image` is the candidate's identity, which is that attempt's fill; the box is
  in fractions of it, and the picture to cut is the loop's own input, of the same size.
- `budget::three_attempts_at_their_largest…` is to be raised to a run of three, and the
  guide's section "What it does not do yet" edited; `runs show` already prints a located
  retry's crops as a count.
- The fixture's judge plays one submission for each check, and a turn that is a tool call
  when the entry is one (`{"tool_call": …}`); `checked::boxed` writes a box on the
  candidate.

**Owed after L1.**

- (Done with the second pull request, after R2's first landed: the install check holds
  the refusal of `whizbang rascal remove-checked` on both targets, at the hook R2 left,
  with its case in `scripts/tests/test-install-check.sh`.)
- `SPEC.md` section 3 still says nothing of a plan in its Prepare bullet (owed since K1),
  and section 7's row "Bounded check and retry" names no action: wording for the task
  that next touches the spec.
- The loop's tests add about a hundred runs of the loop to the fast tier, in memory. If
  the hosted Mac's test time needs it, the first to trim are the second, cached run of
  each case of `table::row_1_…` and `table::row_4_…`.

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
2. **Only the places the table allows.** The places used are those of row 2 findings (L1,
   item 4: a confident violation of `RQ1` that cites nothing that must be preserved), at
   most eight. A place of any other finding is never cut, segmented or painted. A verdict
   with a row 1 finding ends the attempts before this task's code runs; a finding of row
   3, 4 or 5 beside a row 2 finding changes nothing, and its place is not used.
3. **From places to masks.** For each place: the picture the loop was given is cut to the
   place's box, widened by five hundredths of its size on each side (rascal's crop pad),
   with `view`; `segment` is asked the same phrase inside the crop, for at most four
   instances; each mask it gives is carried onto the picture. A place where the segmenter
   finds nothing joins as itself, rasterised: the judge said with confidence that the named
   thing is there, and item 2 has held that it said so of the named thing and of nothing
   that must be preserved. Rascal also halved the segmenter's
   threshold inside a crop; whizbang's `segment` takes no threshold, the phrase is asked as
   it is, and a threshold is milestone E's (below).
4. **The masks are joined as they come, so their number is bounded.** The loop holds one
   found mask, not a list. A located retry gives `select` the found mask of the attempt
   before and the new masks, at most 1 + 8 × 4 = 33 of the 64 `select` takes, by running
   `remove_prompt` from `select` until `select`; what comes back is the one found mask of
   this attempt, and the third attempt starts from it in the same way, again with at most
   33. The shape, the comparison, the fill and the check are L1's, unchanged.
5. **Which retry.** By L1's table and nothing added to it: row 2 findings that cite places
   give the located retry; row 2 findings that cite none give L1's wider mask, once; when
   some cite places and some do not, the located retry is made from those that do.
6. **The budget.** Each attempt after the first adds to the plan's bound at most eight
   requests of the segmenter, which cost nothing on the Mac and will have a price on a GPU
   host (milestone 6), and at most 8 × 3 + 1 calls of the loop's own; the test of the
   largest count (L1) is raised to hold three attempts with eight places each.
7. **The record.** An attempt made by a located retry lists the crops it cut, as boxes on
   the picture, the masks they gave, by identity, and the places that joined as themselves
   because nothing was found there: the fields K1 wrote for it.

- [x] Done when:
  `carry::a_mask_on_a_crop_lands_on_its_picture_and_is_empty_outside_the_crop`,
  `carry::a_mask_on_a_crop_of_another_picture_is_refused`,
  `invert::invert_from_a_crop_mask_lands_protect_on_its_bbox` (rascal's vector),
  `located::a_confident_violation_of_rq1_with_a_place_segments_inside_it_and_the_mask_grows_there`
  (on a check that was made and on the same check answered from the cache),
  `located::a_place_of_a_tentative_violation_or_of_any_other_disposition_is_never_cut_segmented_or_painted`,
  `located::a_place_of_a_finding_that_cites_anything_to_be_preserved_is_never_used`,
  `located::beside_a_violation_of_rq3_the_retry_uses_the_rq1_findings_place_alone`,
  `located::a_place_where_nothing_is_found_joins_as_itself`,
  `located::a_row_2_finding_with_no_place_widens_once`,
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

**Keeping tests lean (L2).** L1's fixture, with a scripted judge that names boxes and, for
each, its obligations, disposition and confidence; the carry is exact arithmetic on masks
of 32 by 32.

**The box stays open for one clause** (L2, 2026-11-20).
`model_tier::a_located_retry_finds_the_phrase_inside_a_crop_with_sam3` is written, is
`#[ignore]`d and is in the model tier's filter, and has not been run: a builder loads no
real model. The box is ticked when `just test-models model_tier` has run it on the Mac.
Every other clause is shown by its test, `just docs` is regenerated, and the row on
`invert`'s `to` is narrowed to its half about a cut canvas. **Ticked after the review**
(2026-11-20): the orchestrator ran the model tier on the Mac at 1adf52f9, and all 9
`model_tier` tests passed, that one among them.

**Where the tests are (L2).** `carry::` and `invert::` are `whizbang-rascal`'s
(`crates/whizbang-rascal/tests/it/`), on a picture of 32 by 32 and on rascal's of 100 by
60. `located::` is a module of the binary's tests on L1's fixture, whose segmenter now
finds what a test says inside a crop, nothing in a crop under a size a test gives, or
fails there; `record::`, `cli::` and `model_tier::` are in the files of those names. The
loop's ways out that L1 left untested are `stopped::`, and the largest run is
`budget::three_attempts_at_their_largest_…`, now made and counted.

**Reworded with the task** (L2, 2026-11-20). These are met otherwise than they are worded.

- Item 1, "offered where rascal offered it, as `invert`'s `to`". Rascal's `to` named a
  frame on the mask's chain. An image here names no parent, so `invert` is given the crop
  the mask is on, beside the image, and the crop's corner as `left` and `top`. "Refused
  when the crop is not of that picture" is held by comparing the two pictures: the crop's
  pixels are the picture's at that corner, or the call is refused. So a crop `view`
  scaled down (a cut over 1,024 pixels on a side) is not carried.
- Item 3, "for each place". A place whose cut would be over 1,024 pixels on a side is not
  looked into, for that reason: a finding that cites only such a place counts as one that
  cites none. A polygon is used by the box around it. A cut is widened to eight pixels
  where it is less, the least `view` cuts. Each has its line in `docs/carry-over.md`.
- Item 7, "lists the crops it cut, as boxes on the picture, the masks they gave". The
  fields K1 wrote hold, for each crop, the crop's identity and one mask or none, and at
  most eight crops. So the record lists each crop by its identity, with the first mask it
  gave, as carried onto the picture, or none where the place joined as itself. The box is
  in the record of the `view` run that cut the crop, and every mask a crop gave is an
  output of its `carry` run; both are among the attempt's runs.
- "`located::a_confident_violation_of_rq1_with_a_place_segments_inside_it_and_the_mask_grows_there`".
  A mask that is no output of the loop is not kept in the store once the run is over, so
  the found mask of a retry cannot be read back. The test holds what `select` was given
  (the found mask of the first attempt and the carried mask), where the cut was made and
  asked, and that the kept mask weighs where the thing was found inside the place and the
  mask of a first attempt that passes, made in a store of its own, does not.
- "`located::a_place_of_a_tentative_violation_or_of_any_other_disposition_is_never_cut_segmented_or_painted`".
  Shown for a tentative violation, `hidden` and `cant_tell`, each with a place, and for a
  tentative violation beside a confident one. A submission cannot say `allowed_change` or
  `observed` of `RQ1` (L1's rewording), and a `held` item cites no place.
- "`located::no_more_than_eight_places_are_used_and_the_plans_bound_covers_them`". A
  ninth place cannot reach the loop through a check: findings hold eight places, and a
  check that cites more hands back findings that are not complete, which end the attempts
  by row 0. The test shows that, and the bound of eight on the loop's own function
  (`checked::located::usable`), given nine.
- Item 5, "row 2 findings that cite places give the located retry", and item 4, "the
  comparison … [is] L1's, unchanged". Changed by the orchestrator's decision after the
  review of the task. The comparison is by the pixels a fill reads, not by identity: a
  place that joins as itself names its place in its mask, and had another identity for
  the same pixels, so a fill and a check were paid for a mask already tried
  (`located::a_place_that_joins_as_itself_inside_the_mask_before_is_not_filled_again`).
  And a located retry whose shaped mask is the one before it no longer ends the
  attempts while the one widening is unused and the plan has an attempt left: the next
  attempt widens the same found mask
  (`located::a_located_retry_whose_mask_is_the_one_before_uses_the_widening_then`).
- Item 3, "a place where the segmenter finds nothing joins as itself". Bounded by the
  orchestrator's decision after the review: only a place of at most a tenth of the
  picture's pixels (`located::ITSELF_PART`); a larger one is not joined, and when nothing
  joined the retry is the wider mask
  (`located::a_place_over_a_tenth_of_the_picture_does_not_join_as_itself`). The number
  is to be looked at again with E1's evidence, and has a carry-over row.
- Item 3 again: two more places are no place. One whose cut is the whole picture, which
  `carry` cannot carry from (`located::a_place_whose_cut_is_the_whole_picture_is_no_place`);
  and any place on a picture that is not flat, which `view` does not cut
  (`located::on_a_panorama_no_place_is_looked_into_and_the_mask_is_widened`).
- `table::row_2_a_confident_violation_of_rq1_with_no_place_widens_once_and_is_checked_again`
  (L1's) no longer ends with a finding that cites a box treated as one that cites none:
  that case is the located retry now.

**Decided with the task (L2).** ADR 0039, amended, has each.

- **The form the carry takes for the loop is an action, `carry`,** a primitive of rascal's
  with its command and its tool record: a run can hand a nested call only an artifact a
  call of its own committed, so the carried masks are a call's outputs.
- **The loop calls six actions** and freezes them: the two recipes, and `view`,
  `segment`, `carry` and the recipe `mask_rect`, which makes a place a mask in one call.
  So a place is three calls, as item 6 counts.
- **A checked plan always states the segmenter's requests,** `segment 1` for one attempt
  and 17 for three; an unchecked plan states none, as before, and its printed line is
  unchanged (the install check holds one).
- **The calls that look into a place are not labelled in the record.** A record holds 32
  labelled steps, and the largest run makes 48 such calls: with labels its record could
  not be read back. The stretches are labelled as L1's are, `a2_select` among them.
- **A wider mask may follow a located retry, and the other way round;** the wider mask is
  still used once in a run. A located retry whose mask cannot be made (a cut refused, a
  segmenter that fails, a carry refused) leaves the attempts before it as they stand.
- **Three attempts at their largest are 185 runs,** made and counted by the test, where
  item 7 of L1 said "under 200": 89 runs, and 96 tool calls three checks may dispatch.
- **The last look at the loop's signal is removed, and the rule it held is the record's:**
  a run that did not complete never says an attempt was kept
  (`crates/whizbang-core/src/contract/record.rs`; ADR 0005). The carry-over row asked for
  a test of each untested branch or its removal; a cancellation between the last call's
  return and that look is a race no test can place.
- **`invert` is version 1.1.0,** since it gained a port and two arguments (after the
  review; it kept 1.0.0 in the first push). What it gives for a mask on its own image is
  unchanged; its mask is now held to the image when the call runs, not when it is
  prepared, because a port belongs to one image and the mask may be on the crop.
- **An attempt the record would not take is the loop's error,** of the type `loop`
  (after the review). No test reaches it: the loop notes at most three attempts, each
  with a reason of its own writing, cut to what a record holds.

**Notes for E1.**

- A record says of each located retry which crops gave a mask and which places joined
  as themselves (`changed.crops`, a null mask). It does not say how many places were
  named and not usable, or looked into and left out for their size: the evidence script
  can count the places in the findings of the attempt before, and the `view` runs of the
  attempt, against the crops. The tenth (`located::ITSELF_PART`) is E1's to confirm.
- `SPEC.md` section 7 names neither the loop nor `carry` (owed since L1, for the task
  that next touches the spec).

## R1. The archive, its checksums and the install check (large)

### Goal

One command makes the archive of the machine's target, and one script proves that an
archive installs and runs, on both runners on every pull request, so that the candidate's
checks hold no surprise (`SPEC.md` section 7, "Release archives for both targets"; section
13, "install from a release archive").

1. **The archive.** `just package` (a script, `scripts/package.sh`) builds the release
   binary as `just smoke` does, with no feature of the tests and without `baseline-replay`
   (ADR 0034), and writes `whizbang-<version>-<target>.tar.gz` holding one folder with
   exactly: the `whizbang` binary; `LICENSE`; `THIRD-PARTY-LICENSES.md` (item 3);
   `COPYRIGHT-library.html`, the notices of the code the Rust standard library itself
   holds, as the toolchain that builds the binary carries them (a fifth file, by the
   orchestrator's decision of 2026-11-19: ship what the licences of the linked code ask
   for; made with R2, and the script fails on a toolchain without it); and
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
4. **The bootstrap of what a user must have**, `scripts/install-prereqs.sh <uv> <folder>`,
   apart from the install check and run before it. `whizbang setup` downloads no Python and
   needs the pinned `uv` (`docs/guides/setup.md`), so a machine with nothing on it cannot
   pass a check that runs `setup`. The bootstrap does what the setup guide tells a user to
   do, and no more: into the folder it is given it puts the `uv` it is handed, held to the
   version `scripts/tool-versions.txt` pins, and the pinned Python, installed by that `uv`
   with the guide's own command. It fetches from nowhere CI does not fetch from today: in
   CI the `uv` is the one the pinned install action already puts on every runner, and the
   Python comes from GitHub's releases by the same command `ci.yml` already runs. No new
   origin is added by this plan ("Origins (R)").
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

- [x] Done when: `scripts/tests/test-package.sh` (the archive holds exactly the four files;
  two builds of one tree give the same list and the same sums line; the sums line matches
  the archive), `scripts/tests/test-install-prereqs.sh` (against a stand-in `uv`: one of
  another version is refused; only the two things are put in the folder; the Python is
  asked of the guide's one origin and no other),
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

**Reworded with the task** (R1, 2026-11-19). Six things are as the task found them.

1. Item 5 says "`whizbang describe --json` equals the committed
   `docs/reference/description.json`". The committed file is `whizbang describe --json
   --pretty` byte for byte (`scripts/docs.sh`), so that is the form the install check
   compares.
2. Item 3 says the licence texts are "generated from the lock file by a script" and that a
   crate without its text "fails `just check`". The file is generated when an archive is
   made and is not committed: licence texts hold their authors' addresses, which no tracked
   file may (`AGENTS.md`, "Secrets and private data"). What fails `just check` is
   `scripts/tests/test-third-party-licences.sh`, which generates the file from the
   workspace's own lock file on every run and holds its crates to those `cargo deny` lists.
   The Rust standard library, which is in the binary and not in the lock file, has its
   two licence texts in the file, taken from the toolchain that builds the binary
   (`share/doc/rust/licenses`). The pinned toolchain carries no `COPYRIGHT`, `LICENSE-MIT`
   or `LICENSE-APACHE` as plain files: the notices of what the standard library itself
   holds are its `COPYRIGHT-library.html`, an HTML page of 1.5 MB, which the file names
   and does not repeat. That is the one gap left, and it is owed below.
3. Item 5's last bullet gives the refusals "On Linux". `whizbang rascal remove-prompt` under
   a config that names the local segmenter is held on both targets, each in its own words:
   on Linux the model does not run on the platform, on a Mac it is not installed yet.
   `whizbang setup --extra torch` is asked on Linux alone.
4. Item 1 says the script writes "a line of `SHA256SUMS`". It writes the file `SHA256SUMS`
   with that one line beside the archive, in `target/package`; R2 joins the two targets'
   lines.
5. Item 2's two tests read the binary cargo builds for the tests, not a release build: the
   same toolchain, libraries and `.cargo/config.toml` link both. The Mac's minimum is set
   there, for every build. A commit costs 2 to 3 seconds of rebuild for the binary's crate
   (measured), so the commit is not kept in a crate of its own.
6. The version line needed one more name in `scripts/check-binary-start.sh`: the binary's
   `src/main.rs` reads the commit its own `build.rs` sets, and nothing else may. The
   workspace's version is still `0.0.1`: it becomes `0.1.0` with R2, item 1, and nothing
   here holds a number.

**Owed after R1** (from the review of #248, 2026-11-19). Three things R1 does not do, the
first two R2's, since R2 makes the candidate's binary:

1. The release binary holds the paths of the machine that built it (cargo's home, the
   checkout's folder) in its panic messages and debug records. R2 builds it with
   `--remap-path-prefix`, so that no archive carries a builder's paths and two machines
   give the same bytes for them.
2. `release::the_linux_binary_needs_only_the_c_library_of_ubuntu_22_04` reads the binary
   built for the tests. R2 runs the same read on the packaged release binary, where the
   candidate is built.
3. The notices of the code the Rust standard library itself holds
   (`COPYRIGHT-library.html` of the toolchain) are named in `THIRD-PARTY-LICENSES.md` and
   not carried in the archive. Whether they go in as a fifth file is a decision that
   changes item 1's four files, so it is asked and not made here.

**Taken with R2** (2026-11-19). All three. 1: `scripts/release-build.sh` builds the
release binary for `just smoke` and for the archive alike, with cargo's home written as
`/cargo` and the checkout as `/whizbang`, and `scripts/package.sh` reads the binary's
bytes and refuses one that holds the checkout's path, cargo's home's or one below the
home folder (`scripts/tests/test-package.sh`). 2: `scripts/package.sh` runs
`scripts/linux-needs.sh` on the binary it archives on the Linux target
(`scripts/tests/test-linux-needs.sh`). 3: decided by the orchestrator: the file goes in,
as item 1 now says; the Done-when's "exactly the four files" is five since then, and
`scripts/tests/test-package.sh` and `scripts/tests/test-install-check.sh` hold five.

**Keeping tests lean (R1).** The script tests of the check run on a tiny stand-in binary
(a shell script named `whizbang`), not on a release build; the real check runs once on each
runner in the `release` job, which has the binary already.

### Origins (R)

None new. The build downloads what CI already downloads: crates from crates.io, the pinned
tools, the pinned Python. The bootstrap uses the `uv` CI already installs and fetches the
Python from GitHub's releases, as `ci.yml` and the setup guide already do. The install
check contacts nothing. Draft 2's bare container, with Ubuntu's package archive and an
image registry, is dropped: a new place to fetch from is the owner's decision, and the
fresh install is shown without one (R2, item 2).

## R2. The `prerelease` and `release` workflows (large)

### Goal

A candidate is built once, checked on both targets and on the owner's Mac, and the bytes
that were checked are the bytes that are released (`SPEC.md` section 14, "GitHub Actions"
and "Workflows").

1. **Versions and tags.** The workspace's version becomes `0.1.0` (the owner's decision
   1). A candidate is the tag `v0.1.0-rc.<n>`; a release is the tag `v0.1.0` on the same
   commit. Only the orchestrator tags, on `main`, after CI and both scans pass on that
   commit (`AGENTS.md`). **How a tag is read.** A candidate tag has two parts: its base
   version, `0.1.0`, and its candidate number, `<n>`. The base version is what is compared
   with the binary's version, and it must be equal; the number is the candidate's own
   identity, kept apart, and is in no binary. So the binary of `v0.1.0-rc.2` says `0.1.0`,
   as the binary of `v0.1.0` does, because they are the same bytes. A tag of any other
   form, or one whose base version is not the binary's, is refused by both workflows and
   by the local leg.
2. **`prerelease`**, on a candidate tag. Jobs:
   - `build`, on `macos-15` and on `ubuntu-22.04`: `just package`; the archive and its sums
     line are kept as the workflow's artifacts.
   - `install`, on each: a fresh job, on the runner itself, that checks out the scripts and
     nothing built, sets up no Rust toolchain, downloads the archive, runs R1's bootstrap
     and then R1's install check. The check's own empty home, cleared environment and
     narrow path are what keep a runner's other software from standing in for something
     the archive or the guide forgot; no container is used, and nothing is fetched from a
     place CI does not fetch from today.
   - `candidate`: when all four pass, makes a GitHub pre-release for the tag, a draft, with
     both archives and `SHA256SUMS`. This is the only job with leave to write, and the only
     thing it may write is a release.
3. **The local leg**, `scripts/prerelease-local.sh <tag>`, the orchestrator's, on the
   owner's Mac, never run by CI. It keeps two things apart that draft 1 ran together.
   - **The checkout's model tests.** `just test-models` builds and tests a checkout, not
     the archive, and the receipt names them so. They, and the list of tests their filter
     names, are run from a checkout the script makes itself, new, of the candidate's
     commit, in a temporary folder, never from the folder the script was started in.
     Before anything is built the script holds that checkout to be clean: its `HEAD` is
     the tag's commit, no tracked file differs from it, and no untracked file is there
     that git does not ignore. A checkout that is not clean stops the leg, whatever its
     `HEAD` says, because a changed or added file is a build input the tag does not hold.
   - **The archive's acceptance.** The Mac's archive is downloaded from the candidate and
     held to `SHA256SUMS`; it is installed into a temporary folder; the installed binary's
     own digest is taken, and its `--version` must name the tag's base version and the
     tag's commit, or nothing more runs. Then, with that binary alone and the owner's env
     file, **in a store made new for this leg, with the decision cache off**, so that
     nothing can be answered from earlier work: `whizbang setup --extra torch`, which
     installs the pack the binary itself carries; `whizbang rascal remove-prompt --until
     feather` on SAM 3; one fill through fal; one `whizbang squint check` by a judge of
     each of the three providers; and `whizbang rascal remove-checked --check required`
     once. Every command that loads a model runs under the heavy
     lock, the thread guard at 200 and the watch on resident memory
     (`scripts/heavy.sh`, `scripts/thread-guard.sh`, `scripts/memory-watch.sh`), as `just
     test-models` does. The pictures are synthetic, drawn by the script.
   - It writes the receipt (item 4) and attaches it to the candidate.
4. **The receipt**, `prerelease-receipt/1`, one file with a fixed shape and no field
   beyond it: the tag, its base version and its candidate number; the commit; the SHA-256
   of each archive and of the installed binary; the binary's version line; that the store
   was new and the cache off; and one entry for each required check, by a fixed name: the
   checkout's model tests; the archive's `setup` with the extra; its mask on SAM 3; its
   fill; its check by each of the three providers, one entry each; and its loop. An entry
   is taken by the script from something a machine wrote, never typed, and holds what
   shows the work was done. Two of the checks are not runs and have no run record, so
   each has evidence of its own kind:
   - **for the checkout's model tests:** `scripts/test-models.sh` is a test run, and what
     it leaves is a test report. The entry says first that the checkout was made new by
     the script and found clean, with its commit and the digest of its tree. The profile `model-tier` (`.config/nextest.toml`) gains
     the report the default profile already writes (`junit`), and the entry holds: the
     commit of the checkout, which must be the tag's; the script's exit status, which must
     be 0; from the report, the names of the tests run, which must be exactly the tests
     the script's filter names at that commit (listed by the same command with its list
     option, before the run), none failed, none timed out, none left out; and the
     report's SHA-256;
   - **for the archive's `setup` with the extra:** `whizbang setup` prints a summary and
     writes no run. The entry holds: its exit status, which must be 0; from what it
     printed, the extra, which must be `torch`, and the worker's hello with its device and
     its thread count, which must be there; and then, from `whizbang backends status
     --probe` by the same installed binary, that the installed pack's worker said hello
     again. No run is named for either.

   The rest are runs, and are read from their records:
   - **for the mask:** the run, completed; the segmenter's model and its version; the
     device;
   - **for the fill and for each provider's check:** the run, completed; the backend, the
     model and its version; that the answer did not come from a cache; **at least one
     request recorded as dispatched to that provider's recipient and settled with an
     answer**; the tokens and what was settled; for a check, the verdict, which may be any
     of the three;
   - **for the loop:** the run, completed; its plan, which must say it checks; the kept
     attempt, checked; **the run of that attempt's check, completed with a verdict**, and
     its own dispatched and settled request.

   It holds no reply text, no path and no key.
5. **`release`**, on a version tag. It builds nothing. The gate,
   `scripts/release-gate.sh`, refuses unless all of these hold for the tag's commit: a
   `prerelease` run on that commit ended in success; the candidate's base version is the
   version tag's; the candidate is the one with the highest number among that version's
   candidates, so a receipt of an earlier candidate that a later one superseded is not
   taken; the candidate holds a receipt that is read whole by its schema; the receipt
   names this candidate and this commit; **every required check is there exactly once and
   meets item 4's conditions for its kind**: for the model tests, exit 0 and a report that
   names every expected test and no failure, from a checkout recorded as new and clean at
   the tag's commit; for `setup`, exit 0, the extra `torch`, and
   the hello heard twice; for a run, completed, not from a cache, a dispatch and a settled
   answer where one is required, the loop checked with its child check completed;
   and the digests the receipt names are those of the archives the candidate holds, which
   the gate takes again from their bytes, and those of `SHA256SUMS`. This is section 14's
   "A release is refused unless this passed on the same commit". The workflow then
   publishes the candidate's own archives and `SHA256SUMS` under the version tag, marked
   as a pre-release.
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

- [x] Done when: `scripts/tests/test-release-workflows.sh` (each workflow runs only on its
  kind of tag; `prerelease`'s jobs call `just package`, the bootstrap and the install check
  and nothing else builds; no job runs in a container or adds a package; only `candidate`
  may write; `release` builds nothing),
  `scripts/tests/test-release-gate.sh` (the release is refused when no `prerelease` run
  succeeded on the commit; when the receipt is missing; when it is malformed or holds a
  field its schema does not; when a required check is missing, is there twice, or did not
  complete; when the model tests' entry does not say its checkout was new and clean, or
  names another commit or tree; when the model tests' exit status is not 0, when their report holds a failed
  or a timed-out test, when it lacks a test the filter names, or when there is no report;
  when `setup` exited with a failure, installed another extra, or printed no hello, or the
  probe heard none; **when a provider's check was answered from a cache, or names no dispatched and
  settled request; when the loop completed unchecked, or its plan did not check, or its
  child check did not complete**; when the store was not new or the cache was on; when it
  names another commit or another candidate; when it is of a candidate a later one
  superseded; when a digest it names is not that of the archive's bytes; and it passes
  when all agree), `scripts/tests/test-prerelease-local.sh` (against stand-ins for the
  binary and for the `gh` command: a wrong sum stops it before anything runs; a binary
  whose version line names another commit stops it; a checkout whose `HEAD` is the tag's
  commit and which holds a changed tracked file, or an untracked file git does not
  ignore, stops it before anything is built; a store that is not empty stops it; a
  failed call is written as failed and fails the leg; a model test that fails, and a
  `setup` that fails, are each written as failed from the report and the exit status and
  fail the leg; each model-loading command is run under the three guards; the loop is asked
  with the check required; the receipt it
  writes is valid by its schema and holds only its fields),
  `scripts/tests/test-tag-version.sh` (`v0.1.0-rc.2` and `v0.1.0` are both accepted for a
  binary that says `0.1.0`, and name the same version; `v0.1.1-rc.1` and `v0.1.1` are
  refused for it; a tag of another form is refused; the candidate number is read apart),
  `scripts/tests/test-skills.sh` and `scripts/tests/test-rules.sh` passing with
  the fifth skill listed in `AGENTS.md`, and the spec's table edited. The first real run of
  `prerelease` is N1's box, not this one.

**Split at its seam** (R2, 2026-11-19). The task passed 2,500 changed lines before the
skill, so it is two pull requests, as "Seams" allows. The first holds items 1 to 5 and
what R1 owed; the second holds item 6, the skill `release` with its row in `AGENTS.md`
and the two tests that hold it. The second also places three carry-over rows from the
review of the first (the orchestrator lifted "R2 closes no row" for those three). Item 7,
the spec's table, waits for the owner's yes to open question 1 and is in neither: **the
box stays unticked after both**, its last clause, "the spec's table edited", being the
one not met, and is ticked by the pull request that makes that edit.

**Item 7 made, and the box ticked** (E1's pull request, #257, 2026-11-20). The owner said
yes to open question 1 on 2026-11-20 ("What the owner decided", 4). `SPEC.md` section
14's row for `prerelease` now says GitHub builds the archive and installs it on both
targets, and that the model tier and the live calls are the local leg on the owner's Mac,
whose receipt the release gate reads; the row for `release` says it publishes the
candidate's two archives, the pack inside the binary, and their `SHA256SUMS`, building
nothing. The other clauses were met by R2's two pull requests (#251, #254).

**Reworded with the task** (R2, 2026-11-19). Eight things are as the task found them.

1. Item 2 says `candidate` is "the only job with leave to write". That is so of
   `prerelease`. The one job of `release` has the same leave, and leave to read the runs:
   it writes the release, and GitHub shows a draft candidate only to a token that may
   write. Neither workflow names a secret but the run's own token.
2. Item 2's jobs take no build cache, and `install` installs `uv` by the pinned install
   action ci.yml uses; the artifacts pass between jobs by GitHub's own upload and
   download actions. A workflow has no trigger but its tag, so neither can be run before
   it is on `main` and a tag is pushed: nothing of either has run.
3. Item 3 says the acceptance runs "with that binary alone and the owner's env file".
   The script never opens the env file: it is named in `WHIZBANG_ENV_FILE` and handed to
   the binary as `--env-file`. The binary is given `HOME`, `PATH` and `TMPDIR` of whoever
   runs the leg, so that `setup` finds `uv` and the Python, and the models directory is
   the one `WHIZBANG_MODELS_DIR` names, which holds SAM 3 already: the leg downloads no
   model. A new store refuses every paid call, so the leg gives it an allowance of 5 US
   dollars first. `setup` with the extra runs before the checkout's model tests, not
   after: it replaces the pack in that models directory with the one the candidate
   carries, and the model tests would otherwise load a model with whichever pack was
   there (from the review of #251). The gate holds the tree the receipt names for the
   model tests' checkout to the tree of the tag's commit, and a candidate that holds a
   receipt already stops the leg before anything is spent.
4. Item 3 says "with the decision cache off". No command has a switch for it: the cache
   is on only under a config with a `[cache]` table, so the leg writes its own config
   without one, and the receipt's `cache` is read back from the records: `on` when any
   of them names the decision cache at all.
5. Item 3 guards "every command that loads a model". The leg guards every command that
   starts the pack: `setup`, the probe, the mask and the loop.
6. Item 4's "request recorded as dispatched to that provider's recipient and settled
   with an answer" is read from three places of a run's record, which has no field of
   that name: a request of that backend that ended `answered`, its line of `spend` at the
   stage `answered`, and the provider's origin among the places data was sent. For a
   check these are in the record of the recipe's `check` step.
7. Item 4's tests run by name are the test's name alone (`model_tier::...`), as the
   report and `cargo nextest list` both give it; `scripts/test-models.sh --list` is the
   list option. The leg is refused unless its own file and the four it reads a tag, a
   folder of files and a receipt with are the new checkout's copies, byte for byte.
8. The loop's command is L1's and is not built. The leg asks it, the receipt requires
   its entry, and until L1 lands every leg fails on it. How the kept attempt's check is
   found in the loop's record (the run of that attempt whose action is
   `source-fidelity`) is marked `L1` in the script for that task to confirm. The install
   check's refusal of `remove-checked` (R1, item 5) is marked `HOOK remove-checked` in
   `scripts/install-check.sh` and is added by whichever of L1 and R2's second pull
   request lands later.

Beside the tests the Done-when names: `scripts/tests/test-release-publish.sh` (the
candidate's and the release's files and the one command that publishes each, against a
stand-in `gh`), `scripts/tests/test-linux-needs.sh`, and the list option in
`scripts/tests/test-test-models.sh`.

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
   (as `scripts/baseline/` is). What the orchestrator runs, exactly. First, once, the
   pictures, drawn again outside the work tree by the command milestone 3 drew them with:

   ```
   whizbang eval edits draw --pictures <the sets' pictures> --out <folder>
   ```

   Then, once for each judge:

   ```
   WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh loop-evidence scripts/loop-evidence.sh \
       --binary <whizbang> --store <a new store> --set <folder> \
       --manifest labs/m3-edits/edits.toml --judge <judge> \
       --config <file> --env-file <file> --out docs/results/m4-loop-<judge>.toml
   ```

   - **The guards are in the script, not left to whoever types the command.** Every command
     it runs that loads a model goes through `scripts/memory-watch.sh` and
     `scripts/thread-guard.sh` at 200 threads, under the heavy lock the line above takes
     (`AGENTS.md`: the watch on memory goes around anything that loads a real model). The
     script refuses to start when it is not under the heavy lock.
   - **Which pictures, decided by the manifest and checked before a cent is spent.** The
     drawn folder also holds clean pictures, two other kinds of defect and the truth masks,
     so the script takes nothing by looking in a folder. It reads the committed manifest
     and takes the entries whose recipe is `remove_prompt`: 24, each naming its picture
     and the picture's SHA-256. Before the first run it checks that there are exactly 24,
     that each file is there and has its hash, and that no entry names a truth mask; on
     any difference it stops, naming the entry, having imported and run nothing.
   - **Then, for each of the 24, in the manifest's order,** it imports the picture
     (`whizbang store import --picture`), runs `whizbang rascal remove-checked` on it with
     the manifest's phrase and the request id `m4-loop-<judge>-<item id>`, so a run that
     was interrupted is run again without repeating a paid call, and then reads the store
     with `whizbang runs export` and writes aggregates only: how many runs; how the
     attempts ended (passed at first, passed after a retry, kept `unsure`, kept `fail`,
     unchecked, error); the attempts made; the retries by kind (wider, located) and the
     endings by the table's row, **among them how often the loop stopped on an `unsure`
     that a retry under rascal's rule would have followed** (a tentative violation of
     `RQ1` with a place), and how often on findings that were not complete; the places cut and how many found nothing; the findings
     by obligation, disposition and confidence; the median time; what was reserved and
     settled.

   The judges are `claude-haiku-5-5` and `gpt-6.1-terra`. The store's config sets the call
   cap at 3.50 dollars, so three attempts are planned with either. The script is tested
   against a stand-in binary; it makes no call a test could pay for.
2. **Three cases by hand**, through the same script, so that they run under the same
   guards: `scripts/loop-evidence.sh --case <name> --image <picture> --prompt <phrase>`
   with the same `--binary`, `--store`, `--config` and `--env-file`, under the same first
   line. The cases are those the carry-over list keeps for milestone 5's playground:
   `pass`, a clean pass, or a retry then a pass; `vague`, a phrase that names nothing in
   the picture, showing its attempts and that it was not passed; `no-key`, an env file
   that lacks the judge's key, leaving the run unchecked and saying why. On pictures of
   the owner's if the owner gives some, else on three of the 24. Their fields (outcome,
   attempts, verdict, why the attempts ended, settled) go in `docs/results/m4-loop.md`,
   which also holds the two tallies.
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

- [x] Done when: `scripts/tests/test-loop-evidence.sh` (against a stand-in binary: each
  of the 24 is run once under its request id; a second run of the script starts nothing
  again; the file written holds aggregates and no item; a set with a picture missing, with
  one changed, with a 25th entry, or with an entry that names a truth mask is refused
  before anything is imported or run; every model-loading command, the three cases among
  them, is run under the watch on memory and the thread guard at 200; without the heavy
  lock nothing starts),
  `evidence::the_loops_record_names_a_commit_of_this_tree_and_holds_only_aggregates` (on
  both judges' tallies and the three cases),
  `evidence::the_loops_tool_record_holds_rung_1_and_says_what_of_rung_2_is_shown`,
  `tool_records::recipes::the_record_of_remove_checked_is_what_it_gives`,
  `scripts/tests/test-tool-records.sh` passing with the new record, and every row of the
  table below closed or re-owned.

**The box stays open** (E1, 2026-11-20, its first pull request). Everything that needs no
paid call is built: the script and its tally with their test, the loop's tool record with
its two tests and its shape check, and the rows. The live record is the orchestrator's runs
and does not exist yet, so
`evidence::the_loops_record_names_a_commit_of_this_tree_and_holds_only_aggregates` holds
the evidence's config and the manifest's 24 entries now, and the two tallies, the page and
the three cases from the moment `docs/results/m4-loop.md` is committed. The box is ticked
by the pull request that brings the record in.

**The live record, and the box ticked** (E1, the same pull request, #257, 2026-11-20).
The orchestrator ran the 24 entries under each judge and the three cases on 2026-11-20 at
48c9251e, with the binary `whizbang 0.1.0` of that commit. The two tallies and the page
are committed (`docs/results/m4-loop-claude-haiku-5-5.toml`,
`docs/results/m4-loop-gpt-6-1-terra.toml`, `docs/results/m4-loop.md`); the three cases'
files are not, since the page holds each whole and the test reads them there. The test
no longer returns early: it fails when the page is missing, and passes on the record.

- **What happened.** All 48 runs completed in one attempt each, and none retried, of
  either kind. Under Haiku 1 passed and 23 kept an `unsure` on row 4; under Terra 18
  passed and 6 kept a `fail` on row 1. No check found a confident violation of `RQ1`.
- **The spend.** 2.281578 dollars settled: 1.045275 under Haiku, 1.153401 under Terra,
  0.042902 for `pass`, 0 for `vague`, 0.04 for `no-key`; against 3.20, 4.10 and 1.50
  estimated ("Spend").
- **The case `pass` did not pass.** It kept an `unsure` in its one attempt: neither "a
  clean pass" nor "a retry then a pass" happened. The page says so, and the case is
  recorded as it ran.
- **The case `vague` ended with another step's refusal.** It ended as the loop's error,
  `invalid`, with nothing spent, saying "the step select (select) was refused: not a call
  of select: the input mask takes 1 to 64, and was given 0". Fixed here, in the recipe
  runner (`crates/whizbang-core/src/recipe/runner.rs`): a step's port that takes at
  least one artifact, bound to an output where an earlier step of the same run committed
  none, ends the run as invalid before the step is called, with "the step segment
  (segment) found nothing, so the step select (select) had no mask to take". One change
  serves `whizbang rascal remove-prompt` and the loop, which hands back the call's own
  type and message (L1, item 8)
  (`checked::a_phrase_that_finds_nothing_ends_remove_prompt_and_the_loop_with_a_sentence_of_its_own`,
  red on the old sentence first). The committed case keeps the sentence measured, with a
  line under it naming the test. Nothing paid was run again.
- **The tenth, and the places** (L2's notes for E1). No located retry occurred in 48 live
  runs; nothing to revisit yet. `located::ITSELF_PART` stays a tenth, and the rows on it,
  on the places a located retry leaves out, and on a carried mask reaching a fill are
  re-owned to milestone E with that line.
- **The owner's pictures move to milestone E** ("What the owner decided", 6): item 2's
  optional part. The loop's retries have no live evidence, and a carry-over row owned by
  E says so.
- **The spec is edited after all,** on the owner's yes ("What the owner decided", 4 and
  5): section 3's Prepare bullet and section 7's rows, as the carry-over row proposed,
  and section 14's two rows of workflows (R2, item 7). The decision above, "The spec is
  not edited here", stood until then.
- **Every clause of the box is shown:** the script's test, the two evidence tests (the
  record test now on both tallies and the three cases), the record test of
  `remove_checked`, `scripts/tests/test-tool-records.sh`, and every row of the table
  below closed or re-owned.

**Decided with the task (E1).**

- **A judge is named as the config names it,** and a backend's name holds no dot. The
  judges are `claude-haiku-5-5` and `gpt-6-1-terra`, the second the model `gpt-6.1-terra`;
  those names are in `--judge`, in the request ids and in the tallies' file names. The
  config is committed, `labs/m4-loop/config.toml`, with the cap at 3.50 dollars and no
  decision cache. A tally names the model each judge's requests were answered by, from
  the records.
- **More than the command above.** `--allowance <USD>` is what a store the script itself
  makes is given, as the local leg of a candidate gives its store one; `--models-dir` is
  handed to the binary; a case takes `--judge` and `--out` too. The page is put together
  by the tally from the two tallies and the three cases' files
  (`scripts/loop-evidence.py page`), which is no paid step and needs no lock.
- **More refusals than the list above.** The binary must say a commit that is this
  checkout's HEAD or an ancestor of it, in a checkout with no tracked change: that commit
  is the one a tally names. And a call that leaves no run stops the script, since nothing
  ran and the next would say the same; the same command then goes on from there.
- **Under the heavy lock is checked, not assumed:** a process above the script must be
  the one the lock names, and the guard's limit must be 200.
- **An entry the store already holds a run for is not asked again;** one that was
  interrupted is asked under its request id, which the store answers.
- **The loop's record test is the binary's**
  (`crates/whizbang/tests/it/tool_records.rs`), not `whizbang-rascal`'s where the other
  recipes' are: only the binary holds rascal's loop and squint's check together. Its name
  is the one above. Its pictures are held pixel for pixel, decoded, not byte for byte.
- **"An `unsure` that a retry under rascal's rule would have followed" is counted as an
  upper bound,** and the tally and the page say so: a run that stopped on an `unsure`
  whose check held a tentative violation of `RQ1` and cited any place. A record keeps a
  check's places apart from its findings.
- **How the attempts ended is counted twice:** by the row of the table the last judged
  attempt's findings are of, worked out from the counts the record keeps; and by the
  sentence the loop wrote, as one of a closed set with `other`, so that no sentence
  reaches a tally (the carry-over row on a closed set of reasons is milestone E's).
- **The spec is not edited here.** The wording owed since K1 and L1 (section 3's Prepare
  bullet, section 7's rows) is in a carry-over row, with N1 as its owner on the owner's
  word.

### What the owner provides (E)

- If the owner wishes: three to five pictures of their own, each with a phrase, where one
  pass leaves something behind (rascal's case was a dog's collar whose buckle stayed), in a
  folder on the Mac. Without them the three cases by hand run on synthetic pictures.

Nothing else. The Google key answers: the gate's fourth run measured the Google judge
whole, with no error (`docs/results/m3-gates.md`, "The fourth run"), so a live call to each
provider can pass.

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
| An executor built with a digest alone | Taken by K1 (item 5), which owns the file `Engine::over` is in |
| `whizbang lab run` prints nothing while a round runs | Taken by A1 (item 7) |
| `whizbang lab run`'s "started now" | Taken by A1 (item 8) |
| The pairs page: a picture's size tells its arm; backspace | Taken by A1 (item 9) |
| The Google rows of prices are introductory | Taken by E1 (item 5) for the row whose date has come by then; each other re-owned to the first live run on or after its date |
| Details first is still `check`'s default (milestone 4's since #246, which re-owned it from E to this plan's first task) | Taken by A1 (items 1 to 6) |
| Hosting the model's files for customers | Its first step, the reading of the licence, taken by E1 (item 6). The rest re-owned to milestone 6 by the owner's decision of 2026-11-19 |

**Rows milestone E owns that milestone 4 leans on.** They stay E's. Two that earlier drafts
listed here are gone: details first as the default is milestone 4's now (above), with the
gating of a fixed details first on fresh data still E's; and the row on the Google arm of
the gate is closed, the fourth run having measured it whole.

| Row | How milestone 4 leans on it |
|---|---|
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
| K1, K2 | Revert K2's merge, then K1's, after L2 and L1. A plan is something only an action that plans has; `from` is an argument no caller gives by default; findings are an addition to what `check` hands back. A store written since holds records of `run/6`, which a build without K1 does not read: the live runs use stores made for them |
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
| A1, K1, K2, L1, L2, R1, R2 | Nothing paid: fakes, and SAM 3 on the Mac under `just test-models` | 0 |
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
A1 ─────────┐
            ├──► L1 ──► L2 ──┐
K1 ──► K2 ──┘                ├──► E1 ──► the candidate ──► N1
R1 ──► R2 ───────────────────┘
```

- A1 before L1: the loop's tests and its cost are those of the default `check`.
- K1 before K2: `check` hands back the kind of result K1 makes, into the record K1 wrote.
- K1 and K2 before L1: the loop is prepared by a plan, runs a recipe in stretches, reads
  findings and fills the record's list.
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
   `pass` hides a leftover. The loop bounds the first (three attempts; only a confident
   finding that the thing remains allows a retry; no `unsure` enlarges a mask) and cannot prevent
   the second. Its mark is experimental, and milestone E takes it up the ladder.
2. **The live step may show almost no retry.** Milestone 3's masks covered the drawn ball
   whole. That is reported as it is; the retries are shown by tests with fakes, and the
   owner's own pictures are the better live evidence.
3. **A judge cites the wrong obligation.** The loop reads a finding by the obligations it
   cites, its disposition and its confidence. A judge that reports a leftover under `RQ2`
   stops a retry that would have helped; one that reports damage as a confident violation
   of `RQ1` alone allows a retry inside that place. The second is the dangerous one, and
   is bounded: the place joins the mask only where the segmenter finds the phrase or,
   finding nothing, as the cited box itself, at most eight places, and `protect` is still
   subtracted by the recipe's own step. E1 counts findings by obligation, disposition and
   confidence, so a judge that confuses them shows.
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
11. **The budgets are estimates.** A pull request that passes about 2,500 changed lines
    splits at its named seam, and the owner is told the count.

## Open questions

Each is phrased so the owner can answer yes or no. The recommended answer is the one the
tasks above assume; a no changes the task named. Three of draft 1's questions are settled
under "What the owner decided", and one of draft 3's under "What the orchestrator decided".

1. **Answered yes, 2026-11-20 ("What the owner decided", 4). The pre-release checks in two parts.** I need your yes to run the model tests and the
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
4. Settled: rascal's retry rules, narrowed so that no `unsure` enlarges a mask ("What the
   orchestrator decided"). The number is kept so that the others keep theirs.
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
    since a diagnostic may not be read as an output. Recommended: yes. (K1, K2, L1)
11. **The loop's record without a score of its own.** I need your yes that milestone 4
    gives the loop rung 1 and rung 2 in part (preservation shown; each attempt's mask and
    fill credited from `remove_prompt`'s record; whether a retry helps reported as counts
    and not scored), with the scoring left to milestone E. No means a task is added that
    teaches the scorer to read a loop's runs. Recommended: yes. (E1)

## What the owner is told, not asked

- **Nine pull requests**, six of them large; three run side by side in the first wave.
- **Details first is turned off as the default now**, because its gate rejected it on all
  three judges and the loop calls `check`; it stays selectable with `--details first`.
- **The tag `m3`**, on 0c75eedd, is made by the orchestrator before the first wave.

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

### Review round 2

On draft 2, at c0b766bd (a local commit; draft 2 was not pushed, because the full check
timed out in a test this change does not touch). Verdict: revise. Ten findings: 8 majors,
2 minors. Each was checked against the code; every one was found right, and all are taken
into draft 3. One line each:

1. **Major: findings left out answers that decide the verdict.** Right: a submission holds
   separate observations with several `obligation_ids`, and their rows enter the verdict
   (`use_case/submission.rs`). Taken: both forms are findings, every cited obligation is
   kept, and the verdict worked out from the findings must be the verdict given (K1, item
   3; K2, items 1 and 3), with a test of an `RQ1` violation beside a separate `SF1`
   violation, on a cache hit and a miss.
2. **Major: an `unsure` could enlarge a mask.** Right: `held` and `allowed_change` at a
   tentative confidence, `cant_tell` and some `hidden` answers all give `unsure`
   (`use_case/mod.rs`, `row`). Taken: one table (L1, item 4) reads a finding by its
   obligations, disposition and confidence; only a confident violation of `RQ1` allows a
   retry; a test for each disposition; open question 4 reworded.
3. **Major: A1 and K1 both changed `engine.rs`.** Right: `Engine::over` is in that file.
   Taken: the row on an executor built with a digest alone is K1's (item 5); A1 and K1
   share no file.
4. **Major: a candidate tag could never equal the binary's version.** Right. Taken: a tag
   is read as a base version, compared with the binary's, and a candidate number, kept
   apart (R2, item 1), with tests of the accepted pair and of a mismatched base.
5. **Major: "passed" did not show the live work was done.** Right: a check can be answered
   whole from the cache, and the loop can complete unchecked. Taken: a new store with the
   cache off; each provider's entry holds a dispatched and settled request, read from the
   run's record; the loop is asked with the check required and its child check must have
   completed; the gate refuses each failure of these (R2, items 3 to 5).
6. **Major: the evidence command had no watch on memory.** Right: `scripts/heavy.sh` runs
   the thread guard and nothing else. Taken: the guards are in the script, at 200 threads,
   for the 24 runs and the three cases, and their use is tested (E1, items 1 and 2).
7. **Major: the pictures were whatever a folder held.** Right: `eval edits draw` writes the
   manifest and does not read it, and its folder holds clean pictures, three kinds of
   defect and truth masks. Taken: the draw command is stated; the 24 are the manifest's
   `remove_prompt` entries, their count and hashes checked before anything is spent, with
   tests of a missing, a changed, an extra and a truth-mask input (E1, item 1).
8. **Major: two new places to fetch from, told and not asked.** Right by `AGENTS.md` and
   `SPEC.md` section 15. Taken: the container is dropped; the fresh install runs on the
   `ubuntu-22.04` runner itself with an empty home and a cleared environment, from origins
   CI already uses; the "told" entry is removed (R1, item 4 and "Origins (R)"; R2, item 2).
9. **Minor: K1 was already over the size for a split.** Right. Taken: K1 in the core and K2
   in `check`, both before L1; nine pull requests.
10. **Minor: `RQ1` beside `RQ3` read two ways.** Right. Taken: the one table; row 5 says a
    violation of `RQ3` forbids nothing, and both tasks have its test.

### Review round 3

On draft 3, at 8b8be872. Verdict: revise. Three findings, all major. Each was checked
against the code; every one was found right, and all are taken into draft 4. One line each:

1. **Major: the first 32 findings could leave out the one that must stop the loop.** Right:
   the submission's schema sets no limit on its answers, and every answer enters the
   verdict. Taken: findings cut short in any way say `complete: false` (K2, item 2), and
   that is row 0 of the table, read first: such findings end the attempts and never
   enlarge a mask (L1, item 4), with tests of an `RQ1` violation kept beside an `SF1`
   violation cut off, and of the only confident violation cut off, on a cache hit and a
   miss.
2. **Major: `--details first` would have done nothing, and the default was not the control
   arm.** Right on both: `door::check_call` leaves an override out when it equals what the
   recipe used to say, and the recipes offer `view,align,resize` in `full` where arm C
   offered `view,align` in `visual_only`. Taken: the default is arm C whole, in the
   recipes, because it is the arm the gate measured; `check_call` states all four settings
   on every call; tests of the command's opt-in, of arms F and C in evaluation, and of the
   direct recipe's and the server's defaults with the tools offered (A1, items 1 to 4).
3. **Major: two receipt entries had no run record to be read from.** Right: the model
   tests are a test run, and `setup` prints a summary. Taken: the model tests' entry is
   the exit status and a test report the `model-tier` profile is made to write, held to
   the filter's own list of tests; `setup`'s is its exit status, its printed extra and
   hello, and a probe that hears the worker again; rejection tests for a failed or
   incomplete test run and a failed setup or hello; no run is named for either (R2, items
   4 and 5).

Also changed with draft 4: draft 3's open question 4 is settled by the orchestrator as the
stricter rule, and E1 counts how often the loop stops on it.

### Review round 4

On draft 4, at a25e2245. Three findings, all major, each a small change. Each was checked
against the code; every one was found right, and all are taken into draft 5. One line each:

1. **Major: a verdict worked out from a cut list can be wrong.** Right: every answer enters
   the verdict, so 32 tentative violations kept with the one confident violation cut would
   read as `unsure` where the verdict is `fail`. Taken: the findings carry the decision of
   the whole submission, never cut; working the verdict out from the findings is held only
   where they are complete; the loop takes its verdict from the carried decision; a test
   of that very case, on a cache hit and a miss (K2, items 1 and 2; L1, item 4).
2. **Major: the model tests could run from a checkout that is not the candidate.** Right:
   `scripts/test-models.sh` builds whatever its folder holds. Taken: the local leg makes a
   new checkout of the candidate's commit and holds it clean (no differing tracked file,
   no untracked file git does not ignore) before it builds or lists a test; the receipt
   records it; a dirty checkout whose `HEAD` is the tag's is refused, with its test (R2,
   items 3 to 5).
3. **Major: two tasks of wave 1 edited `docs/carry-over.md`.** Right. Taken: A1 alone
   edits it in wave 1; the row K1's work answers is closed by K2 in wave 2; the claim that
   A1 and K1 share no file is put right ("Shared files, each with one owner in a wave").

Also changed with draft 5: `main` moved to 0a04cc3c with #246, the record of the gate's
fourth run, the first complete one, which rejected details first on all three judges with
the Google judge answering; the branch holds that merge. The lines that cited the earlier
runs as the latest, or said the Google arm was still owed, say what the fourth run found;
and the row on details first as the default is milestone 4's by that pull request.

**Approved by the orchestrator after four outside review rounds, under the plan-approval
rule (`SPEC.md` section 14, "Review before building"), on 2026-11-19.**
