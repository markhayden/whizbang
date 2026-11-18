# The first verdict: one live call of `squint check source-fidelity`

Plan 2A, task S2, item 5: one call by hand on the owner's Mac, through the binary built
from the commit below (`cargo build --release -p whizbang`, so with no feature of the
tests), under the config of `docs/guides/config.md`, "Checking an edit with a judge", with
the owner's env file, in a store made for it with `whizbang spend reset --allowance 10`.

**The pictures** were made for the call, not taken: a synthetic scene of 640 by 480 (a
sky, a sun, a house, a lawn) as the source, and the same scene with the sky's colours
changed as the candidate. Neither is a photograph, and neither is committed.

The call was made once; it was answered on the first attempt.

```toml
commit = "6f5aaaad5207f7a4f7ea043bd6a3b60fb6ad271d"
whizbang = "0.0.1"
action = "check 1.1.0"
use_case = "source-fidelity"
backend = "anthropic"
adapter_version = "1"
model = "claude-haiku-5-5"
prices_read = "2026-10-26"
run = "1794984359188-1"
outcome = "completed"
verdict = "pass"
turns = "1"
input_tokens = "4151"
output_tokens = "993"
cache_read_tokens = "0"
cache_write_tokens = "0"
reserved_micros = "96384"
request_bound_micros = "6024"
settled_micros = "912"
request_ms = "4982"
run_ms = "5043"
egress_recipient = "https://api.anthropic.com"
egress_classes = "source_images, prompts"
egress_outcome = "sent"
```

**What it shows.** The wire as the live provider speaks it: one request, answered with a
call of `submit_verdict` that the loop read into a verdict, `pass`, with every item of the
checklist confidently held. The ledger settled the turn at its usage, 4,151 input and 993
output tokens at the row of 2026-10-26, 912 millionths of a dollar, against a bound of
6,024 for the request and 96,384 reserved for the call's 16 turns. The input reported is
far under the ceiling of 40,000 the adapter counts against (plan 2A, risk 1). The run's
record lists the one recipient, with the classes it was sent, and its exchange holds no
header.
