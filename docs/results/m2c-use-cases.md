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
