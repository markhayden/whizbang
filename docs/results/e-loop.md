# The loop on segment-01: how often it retried, and its masks against the truth

Plan e1, task L2. The loop `remove_checked` (`docs/reference/tools/remove_checked.md`,
`docs/guides/loop.md`) was run on the 20 pictures of `segmentation/segment-01` of the public
evaluation-pictures repository, pinned by `labs/e-loop/segment-01.toml`, in four groups:
whole 5, parts 6, thin 5, keep-the-hole 4. Two judges, two repetitions each: 80 runs, by
`scripts/loop-evidence.sh`, with SAM 3 on the Mac finding the phrase and Bria Eraser through
fal filling. The judges are `gemini-4-argon`, the loop's judge by J1's ruling (ADR 0044;
`docs/results/e-judges.md`), and `claude-haiku-5-5`, milestone 4's first judge, so the two
are read side by side. The tallies beside this page, `docs/results/e-loop-<judge>-<n>.toml`,
were written by the script; every table below is written from them, and
`evidence::the_loop_records_hold_only_aggregates_by_group` holds the page to them. Only
aggregates are kept, by judge and group: never an item, a phrase, a picture or a word of a
judge.

**What this is.** It is descriptive: the loop's rung 2 in part. It admits nothing. Mask
coverage is the share of the truth mask's pixels inside an attempt's shaped mask where its
weight is at least a half; under-covered is coverage below 0.95, frozen in the plan before
any run. It is a statement about the mask, not about the finished picture: a fill can erase
beyond its mask or leave a trace inside it. The blind contact sheet of the finished
pictures was written outside the repository and given to the owner; it is not marked, and
this record makes no claim about the finished pictures.

## The round

```toml
manifest = "labs/e-loop/segment-01.toml"
set = "segmentation/segment-01"
images_rev = "d5413a4f0806c0c8d4e0376eb71bffe3d541b98e"
commit = "186da513f2ea92e22a80387751c700bedfd638c9"
whizbang = "0.1.0"
judges = ["gemini-4-argon", "claude-haiku-5-5"]
repetitions = 2
runs = 80
settled_usd = 7.069593
```

Run in a new store, the loop store, under the format the format round settled
(`C;submission=v1`; `docs/results/e-submission.md`), each judge's call cap its three-attempt
bound in the binary that ran it, plus a cent: each run reserved 5.926080 dollars under
Argon and 0.458304 under Haiku, and every run planned three checked attempts. A bug of the
script found during the round, that a round run again after one that stopped could not go
on past a picture the store already held, was fixed by 32c873ff before the round was
finished; the binary is of the commit above.

## How many runs retried

Segment-01 reached a retry: under the loop's judge, 26 of its 40 runs made a second attempt,
far over the plan's mark of 5 below which the owner's own photos would have been needed.
Under Haiku, 7 of 40 did.

| Judge | Repetition | Runs | Runs that retried | Retries, wider | Retries, located | Attempts |
|---|---|---|---|---|---|---|
| `gemini-4-argon` | 1 | 20 | 13 | 10 | 8 | 37 |
| `gemini-4-argon` | 2 | 20 | 13 | 9 | 8 | 35 |
| `claude-haiku-5-5` | 1 | 20 | 3 | 0 | 4 | 23 |
| `claude-haiku-5-5` | 2 | 20 | 4 | 1 | 5 | 25 |

Both kinds of retry were made live, under both judges but for Haiku's first repetition,
which made no wider retry. Under Argon, 19 retries were wider and 16 located; under Haiku,
1 and 9. The located retries looked into 16 places under Argon and 10 under Haiku, and
joined every one; in 13 and 8 the segmenter found something there. Every wider retry under
Argon followed a check that cited a place, which a record keeps apart from its findings, so
the place may have been another finding's.

## Retried, by group

Runs that made a second attempt, over both repetitions, of the runs of the group.

| Group | `gemini-4-argon` | `claude-haiku-5-5` |
|---|---|---|
| whole | 3 of 10 | 1 of 10 |
| parts | 9 of 12 | 4 of 12 |
| thin | 8 of 10 | 1 of 10 |
| keep-the-hole | 6 of 8 | 1 of 8 |

## How the runs ended

Over both repetitions, 40 runs a judge.

| Judge | Passed at first | Passed after a retry | Kept `unsure` | Kept `fail` | Unchecked | Error |
|---|---|---|---|---|---|---|
| `gemini-4-argon` | 2 | 2 | 0 | 33 | 0 | 3 |
| `claude-haiku-5-5` | 0 | 0 | 11 | 20 | 7 | 2 |

Argon answered `pass` or `fail` on every check it made, never `unsure`; it found a
confident violation of `RQ1` (the named thing remains) 54 times, the finding that asks for a
retry. Haiku found one 30 times, and answered `unsure` on 11 kept attempts. One run in each
tally ended with no attempt, refused as invalid, and one more under Argon ended in an error
of a step; 7 of Haiku's runs ended unchecked, the check unable to run.

## Why the attempts stopped

The sentence the loop wrote at the end of each run, over both repetitions, as one of the
tally's closed set; reasons no run gave are left out.

| Reason | `gemini-4-argon` | `claude-haiku-5-5` |
|---|---|---|
| `check_could_not_run` | 0 | 7 |
| `damaged_something_preserved` | 0 | 3 |
| `last_attempt_of_the_plan` | 5 | 3 |
| `mask_is_the_one_before` | 1 | 0 |
| `no_attempt` | 3 | 2 |
| `nothing_says_it_remains` | 13 | 24 |
| `passed` | 4 | 0 |
| `wider_mask_already_used` | 14 | 1 |

## Mask coverage across a retry

For each attempt after the first whose shaped mask exists, its coverage against the last
earlier attempt's: up by more than 0.01, within 0.01, or down by more than 0.01; and its
size against that one. Written from `docs/results/e-loop-score-<judge>.toml` once the round
is scored.

| Pending |

## The judge's `RQ1` answer against mask under-coverage

On every checked attempt: a confident violation, a tentative one, or none, against whether
the attempt's shaped mask was under-covered.

| Pending |

## The runs that ended under-covered, and why

The runs whose kept attempt's mask was under-covered, by why the attempts ended; and whether
the kept attempt had the highest coverage of its run.

| Pending |

## The finished pictures

The owner has not marked the contact sheet, so there is no table of "still visible" or
"damaged elsewhere", and this record makes no claim about the finished pictures. If the
owner marks it, the marks stay in owner storage and only the aggregates `eval loop score
--labels` adds are committed here.

## Spend

7.069593 dollars settled in all, in the loop store: 4.855604 under Argon and 2.213989 under
Haiku, against the plan's estimate of about 5 (up to 18 had every run made three attempts
with a dear judge). 0.04 dollars of Argon's second repetition were settled by no recorded
attempt (the tally's `unresolved_micros`).

| Judge | Repetition | Settled, dollars |
|---|---|---|
| `gemini-4-argon` | 1 | 2.434117 |
| `gemini-4-argon` | 2 | 2.421487 |
| `claude-haiku-5-5` | 1 | 1.071020 |
| `claude-haiku-5-5` | 2 | 1.142969 |
