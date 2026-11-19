# The evaluation of milestone 2: arm F over ten sets

Plan 2C, task E1, item 7: on the owner's Mac, through the binary built from the commit
below (`cargo build --release -p whizbang`), the pictures of the five dev sets and the
five adversarial sets fetched by hash from `https://raw.githubusercontent.com` at each
set's pinned revision of `wafflery-evalsets` (`whizbang eval sets fetch`; the owner
approved that origin on 2026-11-18), 110 items, into a store made for the run with
`whizbang spend reset --allowance 5`. Then `matrices/e1-evidence.toml` was run once
(`whizbang eval matrix`): arm F, `claude-haiku-5-5`, two repetitions, two runs at once,
under a config with the judge, `call_cap_usd = 2.00` and the recipients
`https://api.anthropic.com` and `https://raw.githubusercontent.com`, with the owner's
env file. It took 31 minutes (2026-11-18T23:44Z to 2026-11-19T00:15Z).

Only aggregates are kept: by run, and by set. No item's verdict and no reply text.
`agreeing` counts the completed runs whose verdict is the item's label, as the set has it
with its corrections; the labels of these sets are published. Of the 30 runs that failed,
20 ended as `judge_output_ceiling` (a turn cut off at the 2,048 output tokens
`squint check` asks for) and 10 as `invalid_submission` (a second submission that was no
verdict); none was retried, as a matrix does not retry a failed attempt.

`runs stats --tool align` and `runs export --tool align` over the store count the runs of
the action `align`, each a child run of a `check` that measured its pair first: the three
use cases with pairs (source-fidelity, reference-consistency, product-fidelity).

## The run

```toml
commit = "6f155085bdb77e6f43c87676f4783763ddd21e19"
whizbang = "0.0.1"
matrix = "e1-evidence"
matrix_identity = "eeb24af3b9afbd86"
model = "claude-haiku-5-5"
arm = "F"
sets = 10
items = 110
repetitions = 2
decisions = 220
completed = 190
failed = 30
interrupted = 0
pass = 21
fail = 99
unsure = 70
agreeing = 111
retries = 21
repairs = 51
check_time_median_ms = 14935
check_time_p95_ms = 25824
align_runs_by_stats = 172
align_runs_by_export = 172
align_time_median_ms = 400
align_time_p95_ms = 985
settled_micros = 767424
```

## By set

```toml
set = "source-fidelity/scenes-v1"
use_case = "source-fidelity"
items = 20
runs = 40
completed = 37
failed = 3
interrupted = 0
pass = 15
fail = 16
unsure = 6
agreeing = 29
```

```toml
set = "reference-consistency/pets-v1"
use_case = "reference-consistency"
items = 20
runs = 40
completed = 30
failed = 10
interrupted = 0
pass = 2
fail = 14
unsure = 14
agreeing = 15
```

```toml
set = "reality-checks/people-v1"
use_case = "reality-checks"
items = 20
runs = 40
completed = 39
failed = 1
interrupted = 0
pass = 0
fail = 11
unsure = 28
agreeing = 9
```

```toml
set = "brief-compliance/stills-v1"
use_case = "brief-compliance"
items = 20
runs = 40
completed = 36
failed = 4
interrupted = 0
pass = 4
fail = 28
unsure = 4
agreeing = 30
```

```toml
set = "product-fidelity/packshots-v1"
use_case = "product-fidelity"
items = 20
runs = 40
completed = 32
failed = 8
interrupted = 0
pass = 0
fail = 23
unsure = 9
agreeing = 23
```

```toml
set = "source-fidelity/adversarial-v1"
use_case = "source-fidelity"
items = 2
runs = 4
completed = 3
failed = 1
interrupted = 0
pass = 0
fail = 2
unsure = 1
agreeing = 1
```

```toml
set = "reference-consistency/adversarial-v1"
use_case = "reference-consistency"
items = 2
runs = 4
completed = 2
failed = 2
interrupted = 0
pass = 0
fail = 0
unsure = 2
agreeing = 0
```

```toml
set = "reality-checks/adversarial-v1"
use_case = "reality-checks"
items = 2
runs = 4
completed = 4
failed = 0
interrupted = 0
pass = 0
fail = 0
unsure = 4
agreeing = 0
```

```toml
set = "brief-compliance/adversarial-v1"
use_case = "brief-compliance"
items = 2
runs = 4
completed = 3
failed = 1
interrupted = 0
pass = 0
fail = 3
unsure = 0
agreeing = 2
```

```toml
set = "product-fidelity/adversarial-v1"
use_case = "product-fidelity"
items = 2
runs = 4
completed = 4
failed = 0
interrupted = 0
pass = 0
fail = 2
unsure = 2
agreeing = 2
```

