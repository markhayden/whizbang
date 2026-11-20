# 0039. The loop: remove, check, retry

Status: accepted.
Date: 2026-11-20
Spec: section 3 ("Primitives, recipes and loops", "Nested calls", "Cancellation", "What a
run publishes"), section 6 ("Spend"), section 7 ("Bounded check and retry: Kept, as nested
calls"), section 16 row 4; plan m4 (`docs/plans/m4-the-loop-and-a-release.md`), task L1,
with L2 after it; ADR 0004 ("A call's plan"), ADR 0005 ("run/6"), ADR 0008, ADR 0016,
ADR 0028, ADR 0033 ("A run from a step"), ADR 0035

## Context

The spec names a third level of action, the "Loop across tools", gives "remove, check,
retry" as its example, and keeps rascal's bounded check and retry "as nested calls". It
does not say how many attempts a loop makes, what a retry changes, which check judges a
removal, what is handed back when every attempt fails, or what happens when no judge is
ready. Rascal decided each (its ADR 0023, and `crates/rascal-mask/src/heal.rs`, `retry.rs`
and `checker/` at the commit `docs/sources.toml` pins); the old repos are reference, never
authority, so the plan put each to the owner or to its review rounds, and this record
states what was built. Nothing is ported: the loop is new code over what whizbang already
has, and rascal's rules are kept where the plan kept them.

Three things the build already had decide the shape. A recipe file is a straight sequence
(ADR 0033), so a loop is code. The recipe `remove_prompt` fills before it hands anything
back, so a loop that must see a mask before it pays for a fill cannot call it whole; the
runner's `from` and `until` (ADR 0033, "A run from a step") let a caller run one stretch.
And a verdict handed to a caller is a decision and a reason, so what a retry may act on is
the findings `check` hands back (ADR 0035, "What `check` hands back"), never a diagnostic.

## Decision

**The action.** `remove_checked`, of the kind `Loop`, in `whizbang-rascal`
(`crates/whizbang-rascal/src/checked.rs`), because the goal is rascal's. Its inputs are an
image and an optional mask to protect; its arguments are the phrase (`prompt`), the three
backends by name (`backend`, `filler`, `judge`), `attempts` (1 to 3, 3 when not given) and
`check` (`auto`, `required`, `off`; `auto` when not given). It hands back an image and a
mask, and declares a verdict, findings and measurements. `remove_prompt` is unchanged: one
pass, unchecked. It is built over a registry that already holds the two recipes it calls,
and states as its needs what one attempt of theirs needs: `remove_prompt`'s segmenter and
filler, and `source-fidelity`'s judge with the sixteen requests one check may send. Its
plan states the rest.

**What it calls.** Two actions, frozen when it starts (`Context::freeze_steps`): the
shipped recipe `remove_prompt`, and squint's recipe `source-fidelity`. Nothing else, and
nothing under either: the check is given its job, its two images and the judge's name, and
no override of a step, so the tools the judge is offered and its settings are the
recipe's own. What a check needs reaches it as the arguments of a nested call.

**An attempt** is one removal with the check of its result, in at most four calls:

- *Find*, in the first attempt alone: `remove_prompt` until its step `select`. One mask
  comes back, the found mask.
- *Shape*: `remove_prompt` from `grow` until `feather`, given the found mask. The shaped
  mask comes back, the one a fill would paint.
- *Compare*: a retry whose shaped mask has the identity of the attempt's before it is not
  filled, and the attempts end.
- *Fill*: `remove_prompt` from `fill`, given the shaped mask.
- *Check*: `source-fidelity`, the filled picture as the candidate and the picture the loop
  was given as the source.

Every stretch is given the picture the loop was given, so no retry paints over a fill.
Each call is labelled in the record with its attempt and its stretch (`a1_find`,
`a1_shape`, `a1_fill`, `a1_check`, `a2_shape`, …).

**The job** is written from the phrase by a fixed template, `JOB_TEMPLATE`, whose digest
every run notes as the setting `job.template` (`job/1`, ADR 0028). Its brief says what was
to be removed, in the caller's words. Its requirements have fixed ids: `RQ1`, what was to
be removed is no longer visible; `RQ2`, nothing else in the picture changed; `RQ3`, the
filled area looks like its surroundings. The phrase is put into JSON strings as the text
it is: it reaches a judge as the data of a job.

**The table.** A retry paints more of the picture than the attempt before it, so one
table says when one is allowed (`checked::table`), for the wider mask of this task and the
located retry of the next. A finding is read by every obligation it cites, its disposition
and its confidence, and by nothing else, the same for both forms of answer. "Preserved"
means `SF1`, `SF2` or `RQ2`.

| Row | What | What it does |
|---|---|---|
| 0 | The findings say they are not complete | Ends the attempts. Read first: what was cut may be a finding of row 1 |
| 1 | A violation, confident or tentative, that cites anything preserved, whatever else it cites | Ends the attempts, and beats every other row |
| 2 | A confident violation of `RQ1` that cites nothing preserved | Allows a retry |
| 3 | A tentative violation of `RQ1` | Enlarges nothing |
| 4 | Any other disposition, at either confidence | Enlarges nothing |
| 5 | A violation of anything else alone | Enlarges nothing, and forbids nothing |

The verdict of an attempt is the decision its findings carry, never one worked out from
the list. So a `fail` retries only on a confident report that the named thing remains, and
an `unsure` never enlarges a mask: narrower than rascal, which retried on an `unsure` that
named a place (the plan, "What the orchestrator decided").

**A retry, here,** is the wider mask: the shape stretch run again from the same found mask
with its step `grow` given twice the `px` the recipe file states (the runner's override,
`set`), once in a run. A row 2 finding that cites a place is treated as one that cites
none until the located retry lands (plan m4, task L2), which changes how the next
attempt's found mask is made and nothing else here.

**What ends the attempts:** a verdict of `pass`; the last attempt the plan allows; a
verdict the table allows no retry on; a wider mask already used once; a check that could
not run; a retry whose shaped mask is the one before it; a retry whose mask could not be
made or filled, which leaves the attempts before it as they stand.

**The plan** (ADR 0004, "A call's plan") is how many attempts the call may make and
whether it checks, with the reason. For each attempt the bound holds one request of the
filler and every request one check may send its judge; the segmenter is asked once.

- With a judge ready, the plan is the most attempts, up to the number asked, whose whole
  bound is within the config's cap of one call; when that is fewer than asked, the reason
  says how many of how many.
- Under `auto`, when no judge is ready, or the cap covers a removal and no check, the
  plan is one attempt, unchecked, with the reason. A cap that covers no removal refuses
  the call as any call over its cap is refused.
- Under `required`, the judge is a need of the call whatever its state, so a call with no
  judge ready is not ready at prepare, naming it; and a check the cap does not cover is
  planned as one checked attempt and refused as over the cap.
- Under `off`, one attempt, unchecked: no judge is resolved and nothing is reserved for
  one.

The run reads the plan it was prepared with and never works one out.

**One budget.** Admission reserves the plan's bound once. Each nested call takes its
share from the loop's allowance and reserves nothing; what it did not spend is back when
it ends, and the record says what each attempt settled of the loop's allowance, measured
when the attempt ended. A request charged over its bound puts the loop in debt (ADR
0016), and its next call is then refused for want of allowance: the loop starts no
further call, keeps the attempts that completed, and ends with the reason "the budget ran
out after attempt N"; before the first fill has completed it ends as an error of the type
`budget`, with the refusal's own sentence. Three attempts at their largest, each check
dispatching all 32 tool calls a check may, are 133 runs of the 1,024 a tree may hold.

**What is handed back.** The loop completes whenever a fill completed, and commits one
attempt's picture and mask as its own. The attempt kept is the first that passed; failing
that, an `unsure` before a `fail`, and either before an attempt whose check could not run;
then the attempt with fewer findings; then the earlier. With them come the kept attempt's
verdict and findings, when it has them, and a short result as the run's measurements
(`remove_checked/1`: whether the kept attempt was checked, why not when it was not, how
many attempts were begun, which was kept, why the attempts ended). So when every attempt
fails the loop still completes, and says `fail` or `unsure`. It ends as an error only when
no fill completed, with the type and the message the call that failed ended with.

**Unchecked, and saying so.** A check that was started and ended in error leaves its
attempt unchecked, with the check's own error type and message as the attempt's reason,
and the attempts end. The command prints the plan before anything runs, tells each attempt
as it starts and ends, and its last lines say whether the kept attempt was checked, and
why not.

**The record** is `run/6` (ADR 0005): the plan; each attempt with its number, the runs it
made in order, its verdict or the reason it has none, what its retry changed and what its
requests settled; which was kept; and why the attempts ended. An attempt that was begun
is listed, one that made no fill among them: its reason says so, and it is never the one
kept.

**Cancellation.** The loop looks at its signal before each call and starts no other once
it is set; its running child is under its signal. A cancelled loop commits nothing. The
attempts that completed are in its record, and their pictures are names of the loop's own
tree, which no later call can take (ADR 0008).

**Levels.** The loop, squint's recipe, its step `check` and a tool the judge calls are
four, which is every level there is. So a recipe that names a loop as a step is refused
when it is read (`Recipe::check`), in words, and nothing calls a loop.

**The doors** (built by the task's second pull request). `whizbang rascal remove-checked`; the tool `remove_checked` over
`whizbang serve`, whose answer carries the plan, the short result and the verdict;
`whizbang runs show`, which prints a loop's attempts; and `whizbang runs stats`, which
counts loops, their attempts and retries, and why their attempts ended. Exit codes are
those of `whizbang squint check`: a verdict does not change one.

## Consequences

- A caller learns whether a removal was checked from the result itself, and a missing
  judge never blocks a removal unless the caller asks that it should.
- Whatever a retry is allowed to do is decided in one function over data, which the next
  task extends without touching the rule.
- The loop is tied to the shipped recipe's step names (`select`, `grow`, `feather`,
  `fill`) and to the `px` its step `grow` states. It refuses to be built over a recipe
  that lacks them, so a change to the recipe file is caught when the registry is built.
- The check runs as the recipe ships it. A change to what `source-fidelity` measures first
  or offers its judge changes what the loop's check does, with no change here.
- A loop's tool record, its live evidence and its place on the ladder are task E1's.
  Until then it is experimental, and its visual quality is not confirmed by a person.
- To reverse: remove the action from the registry and the command. `remove_prompt` and
  `check` never depended on it.

## Alternatives

- **`remove_prompt` itself checks and retries.** Turned down: a recipe is a straight
  sequence, and a caller who wants one unchecked pass keeps it.
- **A recipe file with a branch and a repeat.** Turned down by the plan: `recipe/1` gains
  neither.
- **Call `remove_prompt` whole and compare afterwards.** Turned down: the fill would be
  paid for before its mask was seen, and an unchanged retry mask would be filled twice.
- **Retry on an `unsure` that names a place,** as rascal did. Turned down by the plan's
  reviews: an `unsure` does not say the named thing is there.
- **Refuse when the cap covers fewer attempts than asked.** Turned down: the spec's spend
  control "errs toward letting work run", and the plan says what it allows before it
  starts.
- **End as an error when every attempt fails.** Turned down: the caller would be handed
  no picture, and the verdict already says what the judge found.
- **State the judge's arm in the loop's call** (the overrides a door states). Turned
  down: a tool that needs an edit evaluated "does not choose squint's tools" (`SPEC.md`
  section 3).
