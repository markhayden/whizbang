# The four other use cases: one live call of each

Plan 2C, task A1, item 6: one call by hand of each of the four use cases that task A1
added, on the owner's Mac, through the binary built from the commit below (`cargo build
--release -p whizbang`, so with no feature of the tests), under the config of
`docs/guides/config.md`, "Checking an edit with a judge", with the owner's env file, in
a store made for them with `whizbang spend reset --allowance 5`.

**The pictures** were made for the calls, not taken: synthetic drawings of 256 by 192 (a
red disc with a white patch, on grey as the reference and on blue as the candidate; a
sky, a sun, a house and a lawn; a red square on grey; a bottle with a blue band, on
white as the reference and on brown as the shot). None is a photograph, and none is
committed. `brief-compliance` was given the brief "A red square on a grey background. No
text and no people."; the other three no brief.

Each call was made once, and answered on the first attempt. The fields of each are
below; no reply text is kept.

## reference-consistency

```toml
commit = "76948d9ea59a0bfe420eb80ab258c323ad568c2c"
whizbang = "0.0.1"
action = "check 1.2.0"
use_case = "reference-consistency"
backend = "anthropic"
adapter_version = "1"
model = "claude-haiku-5-5"
prices_read = "2026-10-26"
run = "1795032706021-1"
outcome = "completed"
verdict = "pass"
turns = "1"
input_tokens = "3728"
output_tokens = "1304"
cache_read_tokens = "0"
cache_write_tokens = "0"
reserved_micros = "96384"
request_bound_micros = "6024"
settled_micros = "1025"
egress_recipient = "https://api.anthropic.com"
egress_classes = "source_images, prompts"
egress_outcome = "sent"
```

## reality-checks

```toml
commit = "76948d9ea59a0bfe420eb80ab258c323ad568c2c"
whizbang = "0.0.1"
action = "check 1.2.0"
use_case = "reality-checks"
backend = "anthropic"
adapter_version = "1"
model = "claude-haiku-5-5"
prices_read = "2026-10-26"
run = "1795032720724-1"
outcome = "completed"
verdict = "unsure"
turns = "1"
input_tokens = "3432"
output_tokens = "788"
cache_read_tokens = "0"
cache_write_tokens = "0"
reserved_micros = "96384"
request_bound_micros = "6024"
settled_micros = "738"
egress_recipient = "https://api.anthropic.com"
egress_classes = "source_images, prompts"
egress_outcome = "sent"
```

## brief-compliance

```toml
commit = "76948d9ea59a0bfe420eb80ab258c323ad568c2c"
whizbang = "0.0.1"
action = "check 1.2.0"
use_case = "brief-compliance"
backend = "anthropic"
adapter_version = "1"
model = "claude-haiku-5-5"
prices_read = "2026-10-26"
run = "1795032724473-1"
outcome = "completed"
verdict = "pass"
turns = "1"
input_tokens = "3357"
output_tokens = "798"
cache_read_tokens = "0"
cache_write_tokens = "0"
reserved_micros = "96384"
request_bound_micros = "6024"
settled_micros = "735"
egress_recipient = "https://api.anthropic.com"
egress_classes = "source_images, prompts"
egress_outcome = "sent"
```

## product-fidelity

```toml
commit = "76948d9ea59a0bfe420eb80ab258c323ad568c2c"
whizbang = "0.0.1"
action = "check 1.2.0"
use_case = "product-fidelity"
backend = "anthropic"
adapter_version = "1"
model = "claude-haiku-5-5"
prices_read = "2026-10-26"
run = "1795032728173-1"
outcome = "completed"
verdict = "unsure"
turns = "1"
input_tokens = "3575"
output_tokens = "1069"
cache_read_tokens = "0"
cache_write_tokens = "0"
reserved_micros = "96384"
request_bound_micros = "6024"
settled_micros = "892"
egress_recipient = "https://api.anthropic.com"
egress_classes = "source_images, prompts"
egress_outcome = "sent"
```

**What it shows.** Each use case, from the command line, through the live provider:
one request, answered with a call of `submit_verdict` that the loop read into a verdict
against the use case's checklist, two passes and two unsure. The ledger settled the four
turns at their usage, 3390 millionths of a dollar in all, against a bound of 6,024 for
each request and 96,384 reserved for each call's 16 turns. Each run's record lists the
one recipient, with the classes it was sent: a reference is sent as a source image is.

## Over stdio, and asked again (task M1)

Plan 2C, task M1, item 5, as the orchestrator narrowed it: one call of `check` by hand
over `whizbang serve`, driven by a small client on its standard input and output, on the
owner's Mac, through the binary built with `cargo build --release -p whizbang` from the
commit below with `main` at 78c96e3 merged into the working tree; then the same call
again, answered from the decision cache. The config was the guide's for Anthropic with
`[cache]`, the owner's env file, and a store made for it with `whizbang spend reset
--allowance 2`. The picture was drawn for the call (a red disc over a blue sky and a green
strip, 256 by 192), not taken, and not committed. `reality-checks`, no brief. The server
exited 0 at the end of its input and wrote nothing to its error output. No reply text is
kept.

```toml door
commit = "af71b4b7f14dc65097bccd21be89a7a08a0cbe2c"
whizbang = "0.0.1"
action = "check 1.3.0"
use_case = "reality-checks"
backend = "anthropic"
model = "claude-haiku-5-5"
model_version = "1+fit1-jpeg90-2576"
run = "1795049187264-1"
outcome = "completed"
verdict = "unsure"
decision_cache = "kept"
turns = "1"
input_tokens = "4581"
output_tokens = "849"
reserved_micros = "96384"
settled_micros = "883"
egress_recipient = "https://api.anthropic.com"
egress_classes = "source_images, prompts"
run_ms = "4358"
call_ms = "4471"
```

```toml door
commit = "af71b4b7f14dc65097bccd21be89a7a08a0cbe2c"
whizbang = "0.0.1"
action = "check 1.3.0"
use_case = "reality-checks"
run = "1795049191738-2"
outcome = "completed"
verdict = "unsure"
decision_cache = "hit"
answered_by_run = "1795049187264-1"
turns = "0"
reserved_micros = "96384"
settled_micros = "0"
egress_recipient = "none"
run_ms = "46"
call_ms = "102"
```

## The Google and OpenAI judges (plan 3, task L0)

Plan 2C, task P1, item 7, re-owned to milestone 3 as task L0: each of the five use cases
asked once of `gemini-3.8-flash` and once of `gpt-6.1-terra`, ten calls by hand on the
owner's Mac, through the binary built from the commit below (`cargo build --release -p
whizbang`), under the config `docs/guides/config.md` gives for each judge ("Checking an
edit with a judge"), with the owner's env file, in a store made for them with an
allowance of 5 dollars. The recipients were approved by the owner on 2026-11-18.

**The pictures** were drawn for the calls by a program, 256 by 192, as for the four above:
a sky, a sun, a house and a lawn, and the same with an orange sky, for `source-fidelity`;
a red disc with a white patch, on grey as the reference and on blue as the candidate; the
scene alone for `reality-checks`; a red square on grey, with the brief "A red square on a
grey background. No text and no people."; a bottle with a blue band, on white as the
reference and on brown as the shot. None is a photograph, and none is committed.

The fields of each run are taken from `whizbang runs export`; no reply text is kept. All
ten completed with a verdict of pass, and the ledger settled 158,735 millionths of a
dollar in all (128,858 for the Google judge, 29,877 for the OpenAI judge), against the
plan's estimate of 0.50 dollars. A run's tokens and its settled spend are summed over its
turns: the record holds no turn's own usage, so the test holds each sum between the
charge of the summed usage at the model's row and that charge plus one millionth for each
turn after the first (each turn is charged its own usage rounded up to a millionth), and
within the bound of a request times the turns taken.

The Google judge's checks took 33 to 149 seconds each against 10 to 14 for the OpenAI
judge's, and more turns (3 to 8 against 3).

```toml judges
commit = "8261a5be5202b9306975e78a5c057d9959d8d69d"
whizbang = "0.0.1"
action = "check 1.4.0"
use_case = "source-fidelity"
backend = "google"
adapter_version = "1"
model = "gemini-3.8-flash"
model_version = "1+fit1-jpeg90-1536"
prices_read = "2026-10-26"
run = "1795070736695-1"
outcome = "completed"
verdict = "pass"
turns = "4"
input_tokens = "13622"
output_tokens = "1589"
cache_read_tokens = "16905"
cache_write_tokens = "0"
reserved_micros = "725760"
request_bound_micros = "45360"
settled_micros = "17445"
egress_recipient = "https://generativelanguage.googleapis.com"
egress_classes = "source_images, prompts, evidence"
egress_outcome = "sent"
seconds = "85.7"
```

```toml judges
commit = "8261a5be5202b9306975e78a5c057d9959d8d69d"
whizbang = "0.0.1"
action = "check 1.4.0"
use_case = "reference-consistency"
backend = "google"
adapter_version = "1"
model = "gemini-3.8-flash"
model_version = "1+fit1-jpeg90-1536"
prices_read = "2026-10-26"
run = "1795070822787-1"
outcome = "completed"
verdict = "pass"
turns = "5"
input_tokens = "16316"
output_tokens = "4910"
cache_read_tokens = "26128"
cache_write_tokens = "0"
reserved_micros = "725760"
request_bound_micros = "45360"
settled_micros = "32612"
egress_recipient = "https://generativelanguage.googleapis.com"
egress_classes = "source_images, prompts, evidence"
egress_outcome = "sent"
seconds = "65.1"
```

```toml judges
commit = "8261a5be5202b9306975e78a5c057d9959d8d69d"
whizbang = "0.0.1"
action = "check 1.4.0"
use_case = "reality-checks"
backend = "google"
adapter_version = "1"
model = "gemini-3.8-flash"
model_version = "1+fit1-jpeg90-1536"
prices_read = "2026-10-26"
run = "1795070888007-1"
outcome = "completed"
verdict = "pass"
turns = "3"
input_tokens = "10287"
output_tokens = "2343"
cache_read_tokens = "5222"
cache_write_tokens = "0"
reserved_micros = "725760"
request_bound_micros = "45360"
settled_micros = "16894"
egress_recipient = "https://generativelanguage.googleapis.com"
egress_classes = "source_images, prompts, evidence"
egress_outcome = "sent"
seconds = "33.2"
```

```toml judges
commit = "8261a5be5202b9306975e78a5c057d9959d8d69d"
whizbang = "0.0.1"
action = "check 1.4.0"
use_case = "brief-compliance"
backend = "google"
adapter_version = "1"
model = "gemini-3.8-flash"
model_version = "1+fit1-jpeg90-1536"
prices_read = "2026-10-26"
run = "1795070921260-1"
outcome = "completed"
verdict = "pass"
turns = "3"
input_tokens = "10017"
output_tokens = "1223"
cache_read_tokens = "5173"
cache_write_tokens = "0"
reserved_micros = "725760"
request_bound_micros = "45360"
settled_micros = "12489"
egress_recipient = "https://generativelanguage.googleapis.com"
egress_classes = "source_images, prompts, evidence"
egress_outcome = "sent"
seconds = "93.1"
```

```toml judges
commit = "8261a5be5202b9306975e78a5c057d9959d8d69d"
whizbang = "0.0.1"
action = "check 1.4.0"
use_case = "product-fidelity"
backend = "google"
adapter_version = "1"
model = "gemini-3.8-flash"
model_version = "1+fit1-jpeg90-1536"
prices_read = "2026-10-26"
run = "1795071014507-1"
outcome = "completed"
verdict = "pass"
turns = "8"
input_tokens = "22704"
output_tokens = "7387"
cache_read_tokens = "62457"
cache_write_tokens = "0"
reserved_micros = "725760"
request_bound_micros = "45360"
settled_micros = "49418"
egress_recipient = "https://generativelanguage.googleapis.com"
egress_classes = "source_images, prompts, evidence"
egress_outcome = "sent"
seconds = "148.9"
```

```toml judges
commit = "8261a5be5202b9306975e78a5c057d9959d8d69d"
whizbang = "0.0.1"
action = "check 1.4.0"
use_case = "source-fidelity"
backend = "openai"
adapter_version = "1"
model = "gpt-6.1-terra"
model_version = "1+fit1-jpeg90-2048"
prices_read = "2026-10-26"
run = "1795071164482-1"
outcome = "completed"
verdict = "pass"
turns = "3"
input_tokens = "3250"
output_tokens = "554"
cache_read_tokens = "5720"
cache_write_tokens = "0"
reserved_micros = "967680"
request_bound_micros = "60480"
settled_micros = "6307"
egress_recipient = "https://api.openai.com"
egress_classes = "source_images, prompts, evidence"
egress_outcome = "sent"
seconds = "14.1"
```

```toml judges
commit = "8261a5be5202b9306975e78a5c057d9959d8d69d"
whizbang = "0.0.1"
action = "check 1.4.0"
use_case = "reference-consistency"
backend = "openai"
adapter_version = "1"
model = "gpt-6.1-terra"
model_version = "1+fit1-jpeg90-2048"
prices_read = "2026-10-26"
run = "1795071179035-1"
outcome = "completed"
verdict = "pass"
turns = "3"
input_tokens = "3379"
output_tokens = "653"
cache_read_tokens = "6051"
cache_write_tokens = "0"
reserved_micros = "967680"
request_bound_micros = "60480"
settled_micros = "6947"
egress_recipient = "https://api.openai.com"
egress_classes = "source_images, prompts, evidence"
egress_outcome = "sent"
seconds = "14.0"
```

```toml judges
commit = "8261a5be5202b9306975e78a5c057d9959d8d69d"
whizbang = "0.0.1"
action = "check 1.4.0"
use_case = "reality-checks"
backend = "openai"
adapter_version = "1"
model = "gpt-6.1-terra"
model_version = "1+fit1-jpeg90-2048"
prices_read = "2026-10-26"
run = "1795071193265-1"
outcome = "completed"
verdict = "pass"
turns = "3"
input_tokens = "2675"
output_tokens = "549"
cache_read_tokens = "4894"
cache_write_tokens = "0"
reserved_micros = "967680"
request_bound_micros = "60480"
settled_micros = "5666"
egress_recipient = "https://api.openai.com"
egress_classes = "source_images, prompts, evidence"
egress_outcome = "sent"
seconds = "12.9"
```

```toml judges
commit = "8261a5be5202b9306975e78a5c057d9959d8d69d"
whizbang = "0.0.1"
action = "check 1.4.0"
use_case = "brief-compliance"
backend = "openai"
adapter_version = "1"
model = "gpt-6.1-terra"
model_version = "1+fit1-jpeg90-2048"
prices_read = "2026-10-26"
run = "1795071206240-1"
outcome = "completed"
verdict = "pass"
turns = "3"
input_tokens = "2604"
output_tokens = "440"
cache_read_tokens = "4774"
cache_write_tokens = "0"
reserved_micros = "967680"
request_bound_micros = "60480"
settled_micros = "5043"
egress_recipient = "https://api.openai.com"
egress_classes = "source_images, prompts, evidence"
egress_outcome = "sent"
seconds = "9.8"
```

```toml judges
commit = "8261a5be5202b9306975e78a5c057d9959d8d69d"
whizbang = "0.0.1"
action = "check 1.4.0"
use_case = "product-fidelity"
backend = "openai"
adapter_version = "1"
model = "gpt-6.1-terra"
model_version = "1+fit1-jpeg90-2048"
prices_read = "2026-10-26"
run = "1795071216216-1"
outcome = "completed"
verdict = "pass"
turns = "3"
input_tokens = "3031"
output_tokens = "522"
cache_read_tokens = "5441"
cache_write_tokens = "0"
reserved_micros = "967680"
request_bound_micros = "60480"
settled_micros = "5914"
egress_recipient = "https://api.openai.com"
egress_classes = "source_images, prompts, evidence"
egress_outcome = "sent"
seconds = "13.1"
```

## The five use cases as recipes (plan m3, task Q1, item 7)

Q1's item 7, reworded with the task: the orchestrator's live run of each use case's recipe
is credited from the gate of details first run again on 2026-11-19, with a binary built at
main `af5392c4dbc9db0caf19dc184c82e7a5b0b4dc43`, after the judges' fixes (#243). Its 1,000
`check` runs are the five recipes on `claude-haiku-5-5` (three repetitions),
`gemini-3.8-flash` and `gpt-6.1-terra`, arms C and F, on the five published dev sets. The
first block is the run, its receipt named by its digest; one block a use case follows, with
its recipe's digest at that commit, its decisions and those that ended in a verdict. Errors
are runs that did not: on `gemini-3.8-flash` nearly every run, which the provider refused
with 402 `RESOURCE_EXHAUSTED` (`docs/results/m3-gates.md`, "Run again"). Spend is what the
store charged, to the cent.

```toml recipes
commit = "af5392c4dbc9db0caf19dc184c82e7a5b0b4dc43"
whizbang = "0.0.1"
action = "check 1.6.0"
gate = "gates/details-first.toml"
receipt = "docs/results/m3-gates/details-first-rerun-receipt.json"
receipt_sha256 = "d5d34a241d3a43a58430f0fee16e9b739caecf64efba6c42e8f8409acea94865"
judges = "claude-haiku-5-5, gemini-3.8-flash, gpt-6.1-terra"
decisions = "1000"
completed = "762"
errors = "238"
pass = "117"
fail = "442"
unsure = "203"
settled_usd = "4.27"
```

```toml recipes
use_case = "source-fidelity"
recipe_digest = "sha256:abf035388252488b0058896669f3d3d6437f97584151c3004a02241a16c8426d"
decisions = "200"
completed = "163"
```

```toml recipes
use_case = "reference-consistency"
recipe_digest = "sha256:a1f1651296bd18d80dd537fc5e89417e7d0db679e51757d30170c381b83d8035"
decisions = "200"
completed = "141"
```

```toml recipes
use_case = "reality-checks"
recipe_digest = "sha256:068fd280b0716c895602d86c95e1e90b5d25c22102f17cba70e8faa39c52e172"
decisions = "200"
completed = "154"
```

```toml recipes
use_case = "brief-compliance"
recipe_digest = "sha256:b2e00ff1ef0418c3d11462fbf4860882d495384b9b82bde58163b89db1375d8a"
decisions = "200"
completed = "159"
```

```toml recipes
use_case = "product-fidelity"
recipe_digest = "sha256:07e0d82abdde420b7291c192fa4fa8e6508c263a8022efcb3c10ee61050c353d"
decisions = "200"
completed = "145"
```

## Details first is off by default (plan m4, task A1)

Every call above that ran through `whizbang squint check` or a use case's recipe ran with
details first, which was the default until plan m4's task A1: the details of each pair
measured before the judge's first turn (ADR 0029), arm `F` of the gate of details first.
That gate (`gates/details-first.toml`) rejected details first in every run of 2026-11-19;
the run to read is the fourth, the first complete one, where it helped on none of the three
judges (`docs/results/m3-gates.md`, "The fourth run, the complete one").

So `check` as a door calls it now is the gate's control arm, whole (arm `C`), the one arm
with evidence behind it: nothing is measured first (`details = none`), the judge is offered
`view` and `align` and no `resize`, and a call of `align` is answered with its pictures and
a fixed sentence (`projection = visual_only`). That is what `whizbang squint check` with
nothing set asks, what the server's `check` asks with no `details`, and what each of the
five recipes says in its own file. Every door states all four settings (the details, the
tools, the projection and the second look) on every call, so none leans on a recipe's file.

Nothing of details first is removed. To turn it on, whole, as arm `F` was measured:

```
whizbang squint check <use case> --details first ...
```

which measures first, offers `resize` too and answers a call of `align` in `full`; over the
server, `"details": "first"`. An evaluation's cell states its arm on `check` itself and is
unchanged, so every committed matrix and gate means what it meant. The recipes' digests
changed with their defaults, so the decision cache misses once for each job. No call of this
record was made again. Details first ends milestone E experimental (plan mE, task A0, item
5): no fix of it was proposed and its gate rejected it in every run, so it is not gated again
on fresh data; `resize`, which `check` offers only under it, ends E experimental with it.
