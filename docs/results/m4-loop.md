# The loop on real models: what happened

Plan m4, task E1, items 1 to 3 (`SPEC.md` section 8, "The ladder"). The loop
`remove_checked` (`docs/reference/tools/remove_checked.md`, `docs/guides/loop.md`) was run
on the 24 entries of `remove_prompt` of the synthetic edit set
(`labs/m3-edits/edits.toml`), once under each of two judges, by
`scripts/loop-evidence.sh`, with SAM 3 on the Mac finding the phrase and Bria Eraser
through fal filling; and three more times by hand. Everything below was read from the
runs' records by `whizbang runs export`. Only aggregates are kept, never an item.

**What this is not.** It scores nothing. No attempt's fill is measured against a clean
picture, and no first attempt is compared with a kept one: the scorer of milestone 3
cannot read a loop's runs. The masks of milestone 3 covered the drawn ball whole, so
most runs were expected to pass at first; the tallies say how many retried, whatever
that number is. That each kind of retry works is shown by the loop's tests with fakes.
Whether a retry makes a result better is rung 2 proper and rungs 3 and 4, and is
milestone E's. The loop's visual quality is not yet confirmed by a person.

**One count is an upper bound.** The loop never retries on an `unsure`; rascal retried on
one that named a place. `unsure_a_retry_under_rascals_rule_would_have_followed` counts the
runs that stopped on an `unsure` whose check held a tentative violation of `RQ1` and cited
any place. A record keeps a check's places apart from its findings, so the place may be
another finding's, and the true number is this or fewer.

## The runs

```toml
commits = "48c9251e4a29bcf4e224e579fd75b09934d02ed4"
tallies = "2"
cases = "3"
settled_micros = "2281578"
```

## What happened

The runs were made on 2026-11-20 at the commit above, with the binary
`whizbang 0.1.0`. Every run of the 48 completed, in one attempt each: no retry of
either kind was made under either judge, so no located retry occurred and no place was
looked into.

| | `claude-haiku-5-5` | `gpt-6-1-terra` |
|---|---|---|
| Completed, in one attempt each | 24 of 24 | 24 of 24 |
| Retries, wider or located | none | none |
| Passed | 1 | 18 |
| Kept `unsure` | 23, all on row 4 (no violation) | 0 |
| Kept `fail` | 0 | 6, all on row 1 (damaged something preserved) |
| Settled | 1.045275 dollars | 1.153401 dollars |

The two judges read the same fills differently. Haiku held `RQ2` tentatively in 12
checks and `RQ3` in 21, and so passed one run of 24: an `unsure` with no violation is
row 4 of the table, which ends the attempts. Terra found a confident violation of `RQ2`
in 6 checks and of `RQ3` in 2, and failed those 6 runs on row 1, which ends them too
(painting more cannot mend a change to what was to be kept). Neither judge found a confident violation of
`RQ1` (the named thing remains), the one finding that asks for a retry, so the loop's
retries were never reached on these pictures.

**Spend.** 2.281578 dollars settled in all: 1.045275 under Haiku, 1.153401 under Terra,
and 0.082902 for the three cases (0.042902 for `pass`, 0 for `vague`, 0.04 for
`no-key`). The plan estimated at most 3.20 dollars under Haiku, 4.10 under Terra and
1.50 for the cases.

## The 24 entries under `claude-haiku-5-5`

```toml
schema = "loop-evidence/1"
commit = "48c9251e4a29bcf4e224e579fd75b09934d02ed4"
whizbang = "0.1.0"
judge = "claude-haiku-5-5"
judge_model = "anthropic claude-haiku-5-5 1+fit1-jpeg90-2576"
filler = "fal bria-eraser 1+fit1-png-binary1-2048"
finder = "pack sam3 sha256:4c1428553b40dcc493f53221fb1134c487432e43f3a39b5a8a5ab5c60f3ed652"
action = "remove_checked 1.0.0"
job_template = "sha256:ec762228fe769d09e6dadebd716e865c643cc095d395fac2f64d4da452d06c40"
config = "sha256:3037828ae7c73a18e08e8db56f842ec9a0dfe58aed020fb3a71bf06134f7eb6d"
manifest_sha256 = "sha256:d1047bd9bf9e8641b528ff2ed2a8e4595fbea9caa79fee76bee16aa9b414faf5"
entries = 24
runs = 24
not_run = 0
attempts_made = 24
unsure_a_retry_under_rascals_rule_would_have_followed = 0
stopped_on_findings_not_complete = 0
median_ms = 41055
least_ms = 31620
most_ms = 50152
reserved_micros = 10999296
settled_micros = 1045275
settled_by_attempts_micros = 1045275
unresolved_micros = 0

[outcomes]
cancelled = 0
completed = 24
error = 0
interrupted = 0
invalid = 0
other = 0

[errors_by_type]

[ended]
passed_at_first = 1
passed_after_retry = 0
kept_unsure = 23
kept_fail = 0
unchecked = 0
error = 0

[plans]
checked = 24
unchecked = 0
none = 0
allowed_1 = 0
allowed_2 = 0
allowed_3 = 24

[runs_by_attempts_made]
made_1 = 24
made_2 = 0
made_3 = 0

[attempt_verdicts]
pass = 1
fail = 0
unsure = 23
unchecked = 0

[retries]
wider = 0
located = 0

[stopped_by_row]
row_0_cut_short = 0
row_1_damaged = 0
row_2_remains = 0
row_3_not_sure = 0
row_4_no_violation = 23
row_5_other_violation = 0
passed = 1
not_checked = 0
no_attempt = 0

[stopped_by_reason]
passed = 1
findings_cut_short = 0
damaged_something_preserved = 0
nothing_says_it_remains = 23
last_attempt_of_the_plan = 0
wider_mask_already_used = 0
mask_is_the_one_before = 0
one_unchecked_pass = 0
check_could_not_run = 0
budget_ran_out = 0
deadline_passed = 0
retry_could_not_be_made = 0
never_said = 0
other = 0
no_attempt = 0

[places]
looked_into = 0
joined = 0
found_something = 0
found_nothing_joined_as_itself = 0
found_nothing_left_out_over_a_tenth = 0
cited_by_checks = 21
cited_as_polygons = 0
retries_wider_after_places_gave_nothing = 0
retries_wider_though_a_check_cited_a_place = 0

[[finding]]
obligation = "RQ2"
disposition = "held"
confidence = "tentative"
count = 12

[[finding]]
obligation = "RQ3"
disposition = "held"
confidence = "tentative"
count = 21
```

## The 24 entries under `gpt-6-1-terra`

```toml
schema = "loop-evidence/1"
commit = "48c9251e4a29bcf4e224e579fd75b09934d02ed4"
whizbang = "0.1.0"
judge = "gpt-6-1-terra"
judge_model = "openai gpt-6.1-terra 1+fit1-jpeg90-2048"
filler = "fal bria-eraser 1+fit1-png-binary1-2048"
finder = "pack sam3 sha256:4c1428553b40dcc493f53221fb1134c487432e43f3a39b5a8a5ab5c60f3ed652"
action = "remove_checked 1.0.0"
job_template = "sha256:ec762228fe769d09e6dadebd716e865c643cc095d395fac2f64d4da452d06c40"
config = "sha256:3037828ae7c73a18e08e8db56f842ec9a0dfe58aed020fb3a71bf06134f7eb6d"
manifest_sha256 = "sha256:d1047bd9bf9e8641b528ff2ed2a8e4595fbea9caa79fee76bee16aa9b414faf5"
entries = 24
runs = 24
not_run = 0
attempts_made = 24
unsure_a_retry_under_rascals_rule_would_have_followed = 0
stopped_on_findings_not_complete = 0
median_ms = 42291
least_ms = 33235
most_ms = 74343
reserved_micros = 72552960
settled_micros = 1153401
settled_by_attempts_micros = 1153401
unresolved_micros = 0

[outcomes]
cancelled = 0
completed = 24
error = 0
interrupted = 0
invalid = 0
other = 0

[errors_by_type]

[ended]
passed_at_first = 18
passed_after_retry = 0
kept_unsure = 0
kept_fail = 6
unchecked = 0
error = 0

[plans]
checked = 24
unchecked = 0
none = 0
allowed_1 = 0
allowed_2 = 0
allowed_3 = 24

[runs_by_attempts_made]
made_1 = 24
made_2 = 0
made_3 = 0

[attempt_verdicts]
pass = 18
fail = 6
unsure = 0
unchecked = 0

[retries]
wider = 0
located = 0

[stopped_by_row]
row_0_cut_short = 0
row_1_damaged = 6
row_2_remains = 0
row_3_not_sure = 0
row_4_no_violation = 0
row_5_other_violation = 0
passed = 18
not_checked = 0
no_attempt = 0

[stopped_by_reason]
passed = 18
findings_cut_short = 0
damaged_something_preserved = 6
nothing_says_it_remains = 0
last_attempt_of_the_plan = 0
wider_mask_already_used = 0
mask_is_the_one_before = 0
one_unchecked_pass = 0
check_could_not_run = 0
budget_ran_out = 0
deadline_passed = 0
retry_could_not_be_made = 0
never_said = 0
other = 0
no_attempt = 0

[places]
looked_into = 0
joined = 0
found_something = 0
found_nothing_joined_as_itself = 0
found_nothing_left_out_over_a_tenth = 0
cited_by_checks = 8
cited_as_polygons = 0
retries_wider_after_places_gave_nothing = 0
retries_wider_though_a_check_cited_a_place = 0

[[finding]]
obligation = "RQ2"
disposition = "violation"
confidence = "confident"
count = 6

[[finding]]
obligation = "RQ3"
disposition = "violation"
confidence = "confident"
count = 2
```

## Three cases by hand

The cases the carry-over list keeps for the playground of milestone 5, run from the
command line through the same script and under the same guards: `pass`, a clean pass,
or a retry then a pass; `vague`, a phrase that names nothing in the picture; `no-key`,
an env file without the judge's key.

### `pass`

```toml
schema = "loop-evidence/1"
case = "pass"
commit = "48c9251e4a29bcf4e224e579fd75b09934d02ed4"
whizbang = "0.1.0"
judge_model = "anthropic claude-haiku-5-5 1+fit1-jpeg90-2576"
filler = "fal bria-eraser 1+fit1-png-binary1-2048"
finder = "pack sam3 sha256:4c1428553b40dcc493f53221fb1134c487432e43f3a39b5a8a5ab5c60f3ed652"
outcome = "completed"
error_type = "none"
error_said = "none"
attempts_asked = 3
attempts_allowed = 3
plan_checks = true
plan_why = "none"
attempts = 1
kept = 1
checked = true
verdict = "unsure"
unchecked_why = "none"
ended = "no finding of attempt 1 says with confidence that the named thing is still there"
ended_as = "kept_unsure"
ms = 35927
reserved_micros = 458304
settled_micros = 42902
unresolved_micros = 0

[[attempt]]
attempt = 1
verdict = "unsure"
unchecked_why = "none"
changed = "nothing"
places_looked_into = 0
places_joined = 0
row = "row_4_no_violation"
runs = 4
settled_micros = 42902
```

Not what the case asks for. The plan's `pass` is "a clean pass, or a retry then a pass";
this run was neither: Haiku answered `unsure` with no violation in its one attempt, row 4
ended the attempts, and the `unsure` was kept. It is recorded as it happened.

### `vague`

```toml
schema = "loop-evidence/1"
case = "vague"
commit = "48c9251e4a29bcf4e224e579fd75b09934d02ed4"
whizbang = "0.1.0"
judge_model = "none"
filler = "none"
finder = "pack sam3 sha256:4c1428553b40dcc493f53221fb1134c487432e43f3a39b5a8a5ab5c60f3ed652"
outcome = "error"
error_type = "invalid"
error_said = "the step select (select) was refused: not a call of select: the input mask takes 1 to 64, and was given 0"
attempts_asked = 3
attempts_allowed = 3
plan_checks = true
plan_why = "none"
attempts = 0
kept = 0
checked = false
verdict = "none"
unchecked_why = "none"
ended = "none"
ended_as = "error"
ms = 5416
reserved_micros = 458304
settled_micros = 0
unresolved_micros = 0
```

The wording was changed after this run: a phrase that finds nothing now ends
`remove_prompt` and the loop, still as `invalid`, with "the step segment (segment) found
nothing, so the step select (select) had no mask to take"
(`checked::a_phrase_that_finds_nothing_ends_remove_prompt_and_the_loop_with_a_sentence_of_its_own`).

### `no-key`

```toml
schema = "loop-evidence/1"
case = "no-key"
commit = "48c9251e4a29bcf4e224e579fd75b09934d02ed4"
whizbang = "0.1.0"
judge_model = "none"
filler = "fal bria-eraser 1+fit1-png-binary1-2048"
finder = "pack sam3 sha256:4c1428553b40dcc493f53221fb1134c487432e43f3a39b5a8a5ab5c60f3ed652"
outcome = "completed"
error_type = "none"
error_said = "none"
attempts_asked = 3
attempts_allowed = 1
plan_checks = false
plan_why = "no judge is ready: the default judge backend is not ready: the env file does not set the variable its `key_env` names"
attempts = 1
kept = 1
checked = false
verdict = "none"
unchecked_why = "no judge is ready: the default judge backend is not ready: the env file does not set the variable its `key_env` names"
ended = "one unchecked pass: no judge is ready: the default judge backend is not ready: the env file does not set the variable its `key_env` names"
ended_as = "unchecked"
ms = 27633
reserved_micros = 40000
settled_micros = 40000
unresolved_micros = 0

[[attempt]]
attempt = 1
verdict = "none"
unchecked_why = "no judge is ready: the default judge backend is not ready: the env file does not set the variable its `key_env` names"
changed = "nothing"
places_looked_into = 0
places_joined = 0
row = "none"
runs = 3
settled_micros = 40000
```
