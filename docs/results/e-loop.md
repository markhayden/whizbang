# The loop on segment-01: how often it retried, and its masks against the truth

Plan e1, task L2. The loop `remove_checked` (`docs/reference/tools/remove_checked.md`,
`docs/guides/loop.md`) was run on the 20 pictures of `segmentation/segment-01` of the public
evaluation-pictures repository, pinned by `labs/e-loop/segment-01.toml`, in four groups:
whole 5, parts 6, thin 5, keep-the-hole 4. Two judges, two repetitions each: 80 runs, by
`scripts/loop-evidence.sh`, with SAM 3 on the Mac finding the phrase and Bria Eraser through
fal filling. The judges are `gemini-4-argon`, the loop's judge by J1's ruling (ADR 0044;
`docs/results/e-judges.md`), and `claude-haiku-5-5`, milestone 4's first judge, so the two
are read side by side. The tallies beside this page, `docs/results/e-loop-<judge>-<n>.toml`,
were written by the script, and the scores, `docs/results/e-loop-score-<judge>.toml`, by
`whizbang eval loop score` from the same store, both repetitions of each judge, 40 runs
requested and none missing, with no marks read. Every table below is written from one or the
other, and `evidence::the_loop_records_hold_only_aggregates_by_group` holds the page to
them. Only
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
size against that one's, by the same margin. Over both repetitions; `all` is the judge's
whole round.

| Judge | Group | Compared | Coverage up | Within | Down | Size larger | Within | Smaller |
|---|---|---|---|---|---|---|---|---|
| `gemini-4-argon` | whole | 3 | 0 | 3 | 0 | 2 | 1 | 0 |
| `gemini-4-argon` | parts | 16 | 2 | 14 | 0 | 5 | 9 | 2 |
| `gemini-4-argon` | thin | 8 | 0 | 8 | 0 | 8 | 0 | 0 |
| `gemini-4-argon` | keep-the-hole | 8 | 4 | 4 | 0 | 8 | 0 | 0 |
| `gemini-4-argon` | all | 35 | 6 | 29 | 0 | 23 | 10 | 2 |
| `claude-haiku-5-5` | whole | 1 | 0 | 1 | 0 | 0 | 1 | 0 |
| `claude-haiku-5-5` | parts | 7 | 1 | 6 | 0 | 2 | 5 | 0 |
| `claude-haiku-5-5` | thin | 1 | 0 | 1 | 0 | 1 | 0 | 0 |
| `claude-haiku-5-5` | keep-the-hole | 1 | 0 | 1 | 0 | 0 | 1 | 0 |
| `claude-haiku-5-5` | all | 10 | 1 | 9 | 0 | 3 | 7 | 0 |

**No retry lowered coverage, under either judge.** Under Argon 6 of 35 retries raised it
and 29 left it within 0.01, though 23 made the mask larger. The gains are in keep-the-hole,
4 of 8, and parts, 2 of 16; none in thin, 0 of 8, or whole, 0 of 3, whose first masks
already covered the truth whole. Under Haiku, which retried rarely, 1 of 10 raised it.

## Mask coverage of the first and the kept attempt

The shaped mask of each run's first attempt and of its kept one: how many were measured, the
mean coverage, and how many were under-covered; and in how many runs the kept attempt had
the highest coverage of its run.

| Judge | Group | First, measured | First, mean | First, under-covered | Kept, measured | Kept, mean | Kept, under-covered | Kept had the highest |
|---|---|---|---|---|---|---|---|---|
| `gemini-4-argon` | whole | 9 | 1.000 | 0 | 9 | 1.000 | 0 | 9 of 9 |
| `gemini-4-argon` | parts | 12 | 0.793 | 8 | 12 | 0.847 | 6 | 6 of 12 |
| `gemini-4-argon` | thin | 8 | 0.920 | 2 | 8 | 0.920 | 2 | 3 of 8 |
| `gemini-4-argon` | keep-the-hole | 8 | 0.744 | 6 | 8 | 0.754 | 6 | 4 of 8 |
| `gemini-4-argon` | all | 37 | 0.860 | 16 | 37 | 0.880 | 14 | 22 of 37 |
| `claude-haiku-5-5` | whole | 10 | 1.000 | 0 | 10 | 1.000 | 0 | 10 of 10 |
| `claude-haiku-5-5` | parts | 12 | 0.793 | 8 | 12 | 0.817 | 7 | 10 of 12 |
| `claude-haiku-5-5` | thin | 8 | 0.920 | 2 | 8 | 0.920 | 2 | 8 of 8 |
| `claude-haiku-5-5` | keep-the-hole | 8 | 0.744 | 6 | 8 | 0.744 | 6 | 7 of 8 |
| `claude-haiku-5-5` | all | 38 | 0.864 | 16 | 38 | 0.872 | 15 | 35 of 38 |

Under Argon the kept masks' mean coverage was 0.880 against the first masks' 0.860, and 14
of the 37 kept masks were under-covered against 16 of the first. The kept attempt had the
highest coverage of its run in 22 of 37 runs; in the other 15 another attempt covered
more: the loop keeps an attempt by its verdict and findings, not by its coverage. Under Haiku the means were 0.872 and 0.864.

## The judge's `RQ1` answer against mask under-coverage

On every checked attempt: whether the judge found a violation of `RQ1` (the named thing
remains), confidently, tentatively or not at all, against whether the attempt's shaped mask
was under-covered.

| Judge | Group | Checked | Confident, under-covered | Confident, covered | Tentative, under-covered | Tentative, covered | None, under-covered | None, covered |
|---|---|---|---|---|---|---|---|---|
| `gemini-4-argon` | whole | 12 | 0 | 5 | 0 | 0 | 0 | 7 |
| `gemini-4-argon` | parts | 27 | 20 | 1 | 0 | 0 | 0 | 6 |
| `gemini-4-argon` | thin | 16 | 4 | 12 | 0 | 0 | 0 | 0 |
| `gemini-4-argon` | keep-the-hole | 16 | 8 | 4 | 0 | 0 | 4 | 0 |
| `gemini-4-argon` | all | 71 | 32 | 22 | 0 | 0 | 4 | 13 |
| `claude-haiku-5-5` | whole | 8 | 0 | 1 | 0 | 0 | 0 | 7 |
| `claude-haiku-5-5` | parts | 18 | 12 | 0 | 0 | 0 | 2 | 4 |
| `claude-haiku-5-5` | thin | 9 | 2 | 0 | 1 | 0 | 0 | 6 |
| `claude-haiku-5-5` | keep-the-hole | 6 | 2 | 0 | 2 | 0 | 2 | 0 |
| `claude-haiku-5-5` | all | 41 | 16 | 1 | 3 | 0 | 4 | 17 |

When the mask was under-covered, Argon found a confident violation in 32 attempts and none
in 4; it never answered tentatively. It also found a confident violation in 22 attempts
whose mask was not under-covered, 12 of them in thin. Under Haiku an under-covered mask
drew a confident violation 16 times, a tentative one 3 times and none 4 times.

## The runs that ended under-covered, and why

Of the runs with a kept attempt, those whose kept shaped mask was under-covered, and why
their attempts ended (the tally's closed set of reasons).

| Judge | Group | Runs with a kept mask | Ended under-covered | Why |
|---|---|---|---|---|
| `gemini-4-argon` | whole | 9 | 0 | none |
| `gemini-4-argon` | parts | 12 | 6 | `last_attempt_of_the_plan` 5, `mask_is_the_one_before` 1 |
| `gemini-4-argon` | thin | 8 | 2 | `wider_mask_already_used` 2 |
| `gemini-4-argon` | keep-the-hole | 8 | 6 | `passed` 4, `wider_mask_already_used` 2 |
| `gemini-4-argon` | all | 37 | 14 | `last_attempt_of_the_plan` 5, `mask_is_the_one_before` 1, `passed` 4, `wider_mask_already_used` 4 |
| `claude-haiku-5-5` | whole | 10 | 0 | none |
| `claude-haiku-5-5` | parts | 12 | 7 | `damaged_something_preserved` 2, `last_attempt_of_the_plan` 3, `nothing_says_it_remains` 2 |
| `claude-haiku-5-5` | thin | 8 | 2 | `nothing_says_it_remains` 1, `wider_mask_already_used` 1 |
| `claude-haiku-5-5` | keep-the-hole | 8 | 6 | `check_could_not_run` 1, `damaged_something_preserved` 1, `nothing_says_it_remains` 4 |
| `claude-haiku-5-5` | all | 38 | 15 | `check_could_not_run` 1, `damaged_something_preserved` 3, `last_attempt_of_the_plan` 3, `nothing_says_it_remains` 7, `wider_mask_already_used` 1 |

Under Argon 14 of 37 runs ended under-covered. The 4 runs that passed are among them, all in
keep-the-hole: the judge found no violation where the mask left more than a twentieth of the
truth uncovered. The other 10 ended `fail`: 5 with the plan's last attempt made, 4 with the
wider mask already used, 1 with a mask that was the one before. So 23 of Argon's 33 runs that ended `fail` had a kept mask that was not under-covered.
Mask coverage cannot say whether those fills left a trace, damaged something else, or the
judge is strict: only the owner's marks on the blind contact sheet can, and this record
makes no claim either way without them.

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
