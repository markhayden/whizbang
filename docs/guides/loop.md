# The loop: remove, check, retry

`whizbang rascal remove-checked` removes what a phrase names from a picture, has the result
checked against the picture it was made from, and tries again a bounded number of times. It
is the action `remove_checked`, the one loop this build ships (`SPEC.md` section 3, "Loop
across tools"; ADR 0039). `whizbang rascal remove-prompt` is still one unchecked pass; the
loop is for a caller who wants to know whether the result was checked, and what the check
said.

```
whizbang rascal remove-checked --image <id> --prompt "leather collar"
        [--attempts <1..3>] [--check auto|required|off] [--protect <id>]
        [--backend <segmenter>] [--filler <filler>] [--judge <judge>]
        --store <dir> --config <file> --env-file <file>
```

The same call over the MCP server is the tool `remove_checked`, with the arguments `image`,
`prompt`, `attempts`, `check`, `protect`, `backend`, `filler` and `judge`
([The MCP server over stdio](serve.md)).

## What it does

An attempt is one removal with the check of its result.

1. **Find.** The segmenter finds what the phrase names, and the instances are joined into
   one mask. This is asked once, in the first attempt.
2. **Shape.** The mask is grown, closed, plugged, kept off `--protect` and feathered, by
   the steps of the recipe `remove_prompt` and with its settings.
3. **Fill.** The filler paints what the mask covers. Nothing is sent to the filler until
   the mask it would paint is made, and a retry whose mask turns out to be the one before
   is not filled a second time.
4. **Check.** Squint's recipe `source-fidelity` holds the filled picture to the picture
   the loop was given, under a job the loop writes from the phrase. The job has three
   requirements: `RQ1`, what was to be removed is no longer visible; `RQ2`, nothing else
   in the picture changed; `RQ3`, the filled area looks like its surroundings.

Every attempt fills the picture the loop was given, never an earlier attempt's result.

**When it tries again.** Only when the judge says, with confidence, that the named thing
is still there, and reports nothing damaged. The retry shapes the same found mask again,
grown twice as far, once in a run. It does not try again when the verdict is `unsure`;
when the judge reports that the removal changed a protected subject, the scene's geometry
or anything else in the picture, because painting more cannot mend that; when the judge's
findings were cut short; or when the last attempt of the plan has been made.

## The plan, said before anything runs

How many attempts the call may make, and whether it checks, is settled when the call is
prepared, and printed on the error stream before anything runs:

```
plan: 3 of 3 attempts asked, checked; fill 3; judge 48
```

That is: three attempts allowed of the three asked, each checked; at most three requests
of the filler; at most 48 of the judge, sixteen turns for each check. What the call
reserves is the bound of all of it, once, and every step draws on that one reservation.
The call then runs under that plan or not at all: a config or a key that changes between
the plan and the run refuses the call as stale.

`--attempts` is the most attempts, the first included: 1, 2 or 3, and 3 when not given.

`--check` says what happens when a check cannot be made.

| `--check` | When no judge is ready, or the cap of one call covers no check |
|---|---|
| `auto`, the default | One attempt, not checked, and the plan says why: `plan: 1 of 3 attempts asked, unchecked; fill 1; judge 0; no judge is ready: …` |
| `required` | The call is not ready, naming the judge that is missing; or it is refused as over the cap |
| `off` | One attempt, not checked. No judge is resolved and nothing is reserved for one, even where one is ready |

## What it costs

A call reserves the bound of its whole plan before any request is sent. The bound of a
check is the bound of one request of the judge at the backend's ceilings (40,000 input
tokens and 4,096 output tokens when the config gives none), times the sixteen turns a
check may take ([The config file](config.md), "What one request may cost"). A fill is one
request of the filler: with the shipped filler, Bria Eraser on fal, a fill reserves 0.04
dollars. The segmenter on the Mac costs nothing. So, in US dollars:

| Judge | One check | One attempt | Two attempts | Three attempts |
|---|---|---|---|---|
| `claude-haiku-5-5` | 0.112768 | 0.152768 | 0.305536 | 0.458304 |
| `gemini-3.8-flash` | 0.725760 | 0.765760 | 1.531520 | 2.297280 |
| `gemini-4-argon` | 1.935360 | 1.975360 | 3.950720 | 5.926080 |
| `gpt-6.1-terra` | 0.967680 | 1.007680 | 2.015360 | 3.023040 |
| `gpt-6.1-sol` | 1.935360 | 1.975360 | 3.950720 | 5.926080 |

No live call was made to work these out: they are computed from the prices the build
holds for each model and from the default ceilings, and a test recomputes each row from
the same tables (`guides::the_loops_guide_states_the_bound_of_an_attempt_for_each_judge_of_the_catalogue`).
They are what a call reserves, not what it spends. Each request is settled at what its
answer says it used, a check that takes two turns of its sixteen gives the rest back, and
an attempt that is not made costs nothing; what a run settled is in its record, attempt
by attempt.

**The cap of one call** (`spend.call_cap_usd`) is held against the bound of the whole
plan. When three attempts would reserve more than the cap, the plan is the most attempts
the cap covers, and it says so:

```
plan: 2 of 3 attempts asked, checked; fill 2; judge 32; the call cap covers 2 of the 3 attempts asked
```

With `gpt-6.1-terra` and a cap of 2.50, that is two attempts (2.01536) and not three
(3.02304). When the cap covers a removal and no check, the plan is one attempt, not
checked, with the reason `the call cap does not cover a check`; with `--check required`
that call is refused instead. A cap that does not cover one removal refuses the call, as
it refuses any call. A cap of one cent covers nothing here: a check alone reserves more.

## What it prints

On the error stream, while it runs: the plan, then each attempt as it starts and as it
ends.

```
plan: 3 of 3 attempts asked, checked; fill 3; judge 48
remove_checked: attempt 1 of 3 starts
remove_checked: attempt 1 ended: fail
remove_checked: attempt 2 of 3 starts
remove_checked: attempt 2 ended: pass
```

On the output, when it is over:

```
run 1795159553507-1 completed
attempts: 2 made, attempt 2 kept
ended: attempt 2 passed
checked: yes
verdict: pass
reason: …
image sha256:…
mask sha256:…
```

`image` is the kept attempt's picture and `mask` the mask it painted, rebound onto that
picture; both are committed to the store.

**Which attempt is kept.** The first that passed. Failing that, an attempt whose verdict
is `unsure` before one that failed, and either before one whose check could not run; then
the attempt with fewer findings; then the earlier one.

**When every attempt fails, the loop still completes.** It hands back its best attempt
with the verdict `fail` or `unsure`, and the caller decides what to do with a picture the
judge did not pass. So read the line `verdict:`, not the exit code, to know whether the
result is good.

**When the result was not checked,** the last lines say so, and why:

```
attempts: 1 made, attempt 1 kept
ended: one unchecked pass: the check is off
checked: no: the check is off
```

A check that was started and ended in an error leaves its attempt unchecked, with the
check's own error type and message as the reason; the picture is still handed back.

**It is an error only when no removal was made:** the segmenter found nothing, the first
fill failed, or the budget ran out before the first fill. The complaint is the type and
the message of the step that failed.

## Exit codes

They are those of `whizbang squint check`: a verdict does not change the exit code.

| Code | Meaning |
|---|---|
| 0 | The loop completed: the kept attempt's verdict, `pass`, `fail` or `unsure`, is printed, or that it was not checked and why |
| 1 | Nothing ran, or no removal was made: the complaint says which, and names the run |
| 2 | The command line was not understood |
| 3 | What was to be printed could not be written |
| 101 | The command itself failed (a panic outside an action): one fixed line is printed |

## The record

`whizbang runs show <run>` prints the loop's attempts after its calls: each attempt's
verdict, or the reason it has none; what its retry changed; how many runs it made; and
what its requests settled; then which was kept and why the attempts ended. Every call an
attempt made is a run of its own under the loop's, shown by its own id. The record also
holds the plan the run was admitted under, the digest of the template the job was written
from, and the kept attempt's verdict and the counts of its findings. No text of a judge's
is in what the loop reads or records of its attempts.

`whizbang runs stats --action remove_checked` counts the loops of a store: their attempts
and retries, how many kept an attempt that was checked, and the runs by why their
attempts ended.

```
loops: 3 runs, 4 attempts, 1 retries; kept an attempt that was checked 2, not checked 1, none 0
  attempts ended	2	attempt N passed
  attempts ended	1	one unchecked pass: the check is off
```

**A loop that is cancelled** commits nothing. The attempts that completed before it are
in its record, and the pictures they made are named there; they are the loop's own, and no
later call can take one.

## What it does not do yet

- A retry aimed at the place the judge named is the next task of the plan (m4, L2). Until
  it lands, a retry widens the whole mask, once. So a call that asks three attempts
  reserves for three and makes two at most; what the third would have cost is given back
  when the run ends.
- The loop's tool record and its live evidence are written with milestone 4's evidence
  (plan m4, task E1). Until then the loop is marked experimental, and its visual quality
  has not been confirmed by a person.
- Nothing may call the loop: calls nest four levels deep and the loop uses all four, so a
  recipe that names it as a step is refused when it is read.
- On Linux no local segmenter runs, so the loop is not ready there until a remote
  segmenter exists (milestone 6).
