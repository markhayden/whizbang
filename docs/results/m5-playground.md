# The loop by hand in the playground: what happened

Plan m5, task E1, items 1 and 2. The by-hand check of the loop `remove_checked` that
milestone 4 left to the playground (`docs/carry-over.md`, "Acceptance checks moved here"),
run live in `whizbang play` in Chrome by the orchestrator, on the binary built at `main`
d62a2182, against the E loop store, with the E loop's config (`labs/e-loop/config.toml`)
under `gemini-4-argon` as the default judge and a call cap of 6.00 dollars. SAM 3 on the Mac
found the phrase, Bria Eraser through fal filled, and Argon checked. Each run's figures were
read from its record and from `whizbang spend status`. Only aggregates and run ids are kept
here, never an item of the set, its phrase, a picture, a path or a key; each result screen's
screenshot is kept out of the repository.

**What this is not.** It scores nothing. It shows that the playground runs the loop, shows
how each run ended and why, refuses a run it cannot make, and accounts for a quit, as a
person sees them. Whether a retry makes a result better, and whether Argon is right, are
milestone E's.

## The runs

```toml
commit = "d62a2182137dac635014d6437b1df4a9ac2f2bb1"
whizbang = "0.1.0"
runs = 5
settled_micros = 176000
unresolved_micros = 40000
```

Five runs were made, and one more case started none. They settled 0.176 dollars, and 0.04
dollars more is booked as unresolved: the bound of the fill a quit cut off (below). The
store's books show no reservation left open.

## The cases by hand

The cases of the carry-over list: `pass`, a clean pass, or a retry then a pass; `vague`, a
prompt that names nothing in the picture, showing how it ends; and an unset key, in two
forms: `unchecked`, no judge's key with the filler's set, which runs unchecked and says why,
and `no-key`, an env file with no key at all, which is refused before a run starts. Beside
them, `fail`, the first run made, kept because it shows the loop's stop.

### `fail`

```toml
case = "fail"
run = "1795342659824-1"
outcome = "completed"
verdict = "fail"
attempts = 1
seconds = 77
settled_micros = 66000
reserved_micros = 5930000
```

A removal on a picture of the E loop's set ended FAIL on its first attempt, at 0.066
dollars (a fill, 0.040 on fal; two judge turns, 0.026 on Google) of 5.93 held. The loop
stopped by design and did not retry: "attempt 1 changed something that must be preserved,
which painting more cannot mend". The named object was removed and a part fixed to it was
left, at the check's third place; Argon held that part as something to preserve. That is a
question of the judge for milestone E, not of the playground.

### `pass`

```toml
case = "pass"
run = "1795342796956-18"
outcome = "completed"
verdict = "pass"
attempts = 1
seconds = 79
settled_micros = 70000
reserved_micros = 5930000
```

The same task on another picture of the set, picked from the library, passed on its first
attempt at 0.070 dollars: the clean pass the carry-over list asked for. One part of the
named object was left in the picture and Argon passed the run, a concrete case of the judge
round's finding that Argon passes runs whose mask left part of the object
(`docs/results/e-judges.md`).

### `vague`

```toml
case = "vague"
run = "1795342910635-35"
outcome = "error"
checked = false
seconds = 1.4
sent = "nothing"
settled_micros = 0
reserved_micros = 5930000
```

The prompt "the thing" on the same picture: the segmenter found nothing, so the run stopped
in 1.4 seconds with nothing sent off the machine and nothing spent, its 5.93 dollars held
released. It ended as an error, its child `remove_prompt` invalid. The page said so in
words a person does not use, and its canvas stayed loading (both found live, below).

### `unchecked`

```toml
case = "unchecked"
run = "1795345304261-1"
outcome = "completed"
checked = false
attempts = 1
seconds = 74
settled_micros = 40000
reserved_micros = 40000
```

The same store and binary, under a copy of the config that pointed every judge at a key
variable the env file does not set, so the filler had its key and no judge did. The plan
showed one unchecked fill held at 0.040 dollars, and Run was offered. The run ended DONE, not
PASS, in 1 minute 14 seconds, at 0.040 dollars of 0.040, the fill alone. Its summary says
"one unchecked pass: no judge is ready", with the reason: the run left unchecked, saying
why, that the carry-over list asked for.

### `no-key`

```toml
case = "no-key"
run = "none"
sent = "nothing"
settled_micros = 0
```

A second playground on a new store, with an empty env file. The status bar said "no key"
for the filler and for every judge, and Run stayed unavailable with the reason beside it, so
no run was started and nothing was sent. Quit, with nothing running, stopped it. The
playground refuses a call it cannot make before it starts a run, so the form "a run left
unchecked, saying why" is reached only with the filler's key set, as in `unchecked` above.

## A quit during a paid fill

### `quit`

```toml
case = "quit"
run = "1795343001136-38"
outcome = "cancelled"
seconds_into_fill = 8.4
open_reservations = 0
store_took_a_writer = true
settled_micros = 0
reserved_micros = 5930000
unresolved_micros = 40000
```

One loop started from the page, and Quit pressed 8.4 seconds into its fill. The question said
one run going would be cancelled; the page then said the host had stopped, and the run's
record says cancelled. `whizbang spend status` named no reservation left open: the run's
5.93 dollars were released, and the fill request in flight was booked at its bound, 0.04
dollars, as unresolved (`whizbang spend status --run` shows it). The store was opened to
write straight after, in under a tenth of a second, and the command exited 0.

## Found live

Each was found in these runs and is fixed by the pull request of the branch
`m5-live-fixes`:

- The top bar shows what is left as if it were spent, never reads it again, and is missing
  on a new store.
- Compare, side by side, clips the picture after.
- Coming back to a task after a run sometimes shows its lists of runs and masks empty.
- The form shows NOT READY for a moment before its first preparation.
- With no key, the segmenter on the Mac is marked NO KEY, the plan calls the fill that is
  not ready "local" and "free", and the reason is in the code's words.
- A vague prompt's message is in the code's words, and the canvas stays loading.
- `whizbang runs show` of a cancelled run says 0.00 dollars while its child has 0.04
  unresolved.
- The rows of the picker of what to keep out cannot be told apart.
- An upload answered 429 is not tried again.
- The run's page asks for news about four times a second.
- A run left unchecked gives its reason in the code's words, twice, and its result still
  names the judge though no judge ran.

A follow-up, not fixed: Home's library lists only the pictures given through the
playground, so a store filled from the command line shows "No pictures yet".
