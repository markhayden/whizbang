# Milestone E, slice 1 note

The first slice of milestone E, "the default judge, the submission format, the loop's
retries", ends with task L2 of `e1-judge-format-loop.md`. This note is written by L2's pull
request from its branch, after the three rounds were run and the loop round was scored.
Its slice ran beside milestone 5, as decided with the owner on 2026-11-20
(`docs/plans/m4-note.md`).

## The three outcomes

- **The submission format: v1 stays.** The format round ran the three small judges under v1
  and v2 on the ten dev sets, two repetitions, 1,320 cells (`docs/results/e-submission.md`).
  `submission/1` did not adopt v2, by clause (c): submission failures fell from 14 to 4, and
  no judge had more under v2, but each of the three moved by more than 2 points on accuracy
  or a false-reject rate. v2 stays a setting no default uses (ADR 0042).
- **The default judge: `gemini-4-argon`.** The judge round ran all five judges under
  `C;submission=v1` on the same 253 items (the ten dev sets and `blind-v3`), and
  `judge-pick/1` picked Argon over all sets pooled (`docs/results/e-judges.md`; ADR 0044).
  Over the `source-fidelity` sets alone the rule made no pick, and the loop uses Argon by
  ruling. The guides' first example names it.
- **The loop's retries: reached live.** The loop round ran on the 20 pictures of
  `segmentation/segment-01`, under Argon and under `claude-haiku-5-5`, two repetitions each,
  80 runs (`docs/results/e-loop.md`). Under Argon 26 of 40 runs retried, with 19 wider and 16
  located retries; under Haiku 7 of 40, with 1 and 9. Both kinds of retry were made live for
  the first time. Scored against the hand-drawn truth masks (`whizbang eval loop score`;
  `docs/results/e-loop-score-<judge>.toml`):
  - Under Argon a retry never lowered mask coverage: of 35 retries compared with the
    attempt before, 6 raised it, 29 left it within 0.01 and none lowered it. The gains are
    in keep-the-hole (4 of 8) and parts (2 of 16), none in thin (0 of 8) or whole (0 of 3).
  - The kept masks' mean coverage was 0.88 against the first masks' 0.86, and 14 of the 37
    runs with a kept mask ended under-covered.
  - When the mask was under-covered, Argon found a confident violation of `RQ1` in 32
    attempts and none in 4; those 4 are the 4 runs that passed, all in keep-the-hole.
  - 23 of Argon's 33 runs that ended `fail` had a kept mask that was not under-covered.
    Mask coverage cannot say whether those fills left a trace or the judge is strict: only
    the owner's marks on the blind contact sheet can, and the record makes no claim either
    way without them.
  - Haiku retried rarely: 1 of its 10 retries compared raised coverage, and none lowered it.

All three are descriptive and admit nothing: the gate on fresh data that would admit a
default judge or a format is owed (below).

## What shipped

| Pull requests | Task | What |
|---|---|---|
| #260 | Plan | The slice's plan, after four rounds of outside review |
| #262 | T1 | Every rejection of a submission kept by its class, and why the run ended, on every exit of `check` |
| #266 | S1 | `whizbang eval judges`, and the rules `judge-pick/1` and `submission/1` as code |
| #263 | L1 | The loop on segment-01: its pinned manifest, the evidence script generalised, `eval loop score` and `eval loop sheet` |
| #264 | F1 | Submission v2, a setting no default uses |
| #267 | F2 | The format round and its decision: v2 not adopted (clause c) |
| #270 | J1 | The judge round, its record, and the default judge (ADR 0044) |
| This one | L2 | The loop round's tallies, scores and record; `eval loop score` reading a grey truth mask with a colour profile; this note |

Each record is held to its numbers by a test: `evidence::the_submission_record_holds_only_aggregates`,
`evidence::the_judges_record_holds_only_aggregates` and
`evidence::the_loop_records_hold_only_aggregates_by_group`.

## Spend

About 36.12 US dollars settled of the milestone's 100, all to approved recipients, each
settled by a store's ledger, against the plan's expectation of about 46 ("Spend").

| Round | Store | Settled, dollars | Plan's estimate, dollars |
|---|---|---|---|
| F2, `e-submission` | The evaluation store | 11.77, and 0.023 for a probe of a provider | 19.0 |
| J1, `e-judges-dev` and `e-judges-blind` | The evaluation store | 17.26 | 22.3 (7.0 and 15.3) |
| L2, the loop round | The loop store | 7.07 | 5 (up to 18) |
| The owner's photos | | Not run | 2 |
| A round run again after a fix | | None | 10 |

T1, S1, L1 and F1 paid for nothing: they ran on fakes. No round passed twice its estimate.
The loop round settled more than its estimate and far less than its bound: under Argon most
runs made two or three attempts, each with a fill and a check.

## Test time

This pull request's `just check`, on the owner's Mac with six test threads: 3,216 fast tests
passed in 218.051 seconds of test time, under the limit of 300. The plan allowed the slice
at most 10 seconds of fast-tier test time; this pull request's two new tests took 0.057
seconds together (`loop_score::a_grey_truth_mask_with_a_colour_profile_is_read_by_its_grey_samples`,
`evidence::the_loop_records_hold_only_aggregates_by_group`). The earlier tasks' tests were
timed by their own pull requests and are not measured apart here. The first run of this
check was stopped by a crash test that timed out under load (below).

## The owner's contact sheet

The blind contact sheet of the loop's finished pictures (`whizbang eval loop sheet`) was
written to the owner's storage, outside the repository, and nothing of it is committed. The
owner may mark it later; until then the records make no claim about the finished pictures,
only about the masks.

## What a reader should not assume

- **Nothing here is admitted.** The default judge and the format are chosen on dev sets and
  an exposed set, and the loop's figures are descriptive.
- **Argon's lead comes from `blind-v3`.** Pooled it is the pick; on the dev sets alone it
  trails `gemini-3.8-flash` (`docs/results/e-judges.md`).
- **Mask coverage is not the finished picture.** The loop's scores measure masks against
  hand-drawn truth masks, whose edges are imprecise; whether a finished picture still shows
  the named thing waits on a person's marks.
- **The two loop judges behave differently.** Argon never kept an `unsure` and failed most
  runs after retrying; Haiku retried rarely, kept `unsure` 11 times and could not check 7
  runs. Neither passed often: Argon 4 runs of 40, Haiku none.

## What moved to the carry-over list

Narrowed: the loop's retries, wider and located, have no live evidence. Both kinds were made
live and recorded, and what each retry did to the mask is scored beside the attempt before
it; what is left is what it did to the finished picture, owed to the owner's marks.

Opened by this note:

- the gate on fresh data, owed to the default judge (Argon's lead comes from `blind-v3`; on
  the dev sets alone it trails Flash) and to v2;
- the owner's photos, not used: segment-01 retried, so they are optional;
- the evaluation-pictures repository's old name, still named by `SETS_REPOSITORY` and each
  set's `images_repo`;
- `scripts/loop-evidence.sh` could not go on after a round that stopped: fixed by 32c873ff,
  closed;
- `eval loop score` refused a grey truth mask with a colour profile: fixed in this pull
  request (`loop_score::a_grey_truth_mask_with_a_colour_profile_is_read_by_its_grey_samples`),
  closed;
- tests that failed at random during the slice: the pack worker's test of a request it
  does not read, on the clock (fixed by #276), and a cancel test of `play_models`, whose fix
  is being made; and a third, seen once by this pull request's check, a crash test of the
  model installs that timed out under load and passed alone.

## What the next slice of E owes

The loop's finished pictures judged by a person, from the owner's marks of the contact
sheet; its visual quality confirmed by a person; the gate on
fresh data for the default judge and the format; the blinded picking rounds of milestone 3;
`tiles` and the second look; details first gated again on fresh data.
