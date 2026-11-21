# 0042. Submission v2

Status: accepted.
Date: 2026-11-21
Spec: section 3 ("What every run records"), section 7 (the use cases), section 8 ("Marks"),
section 16 (row E), section 17 (the submission "ported as is, then redesigned through the
ladder"); plan `docs/plans/e1-judge-format-loop.md`, task F1

## Context

`check` under a use case of the table asks its judge for one structured answer, the tool
`submit_verdict`, ported from squint byte for byte: a list of items, each naming its checklist
id, and a list of findings. A submission that breaks a rule is answered with the first
problem found and asked for once more; a second rejection ends the run as
`invalid_submission`. Small models fail this form: in milestone 2's evaluation Haiku needed
51 repairs in 220 decisions and ended 10 as invalid submissions
(`docs/results/m2c-evaluation.md`). Task T1 of the same plan records each rejection by class
and each run's cause of ending (`rejections.json`), so the effect of a new form can be
counted.

## Decision

1. **A setting, `submission`, `v1` or `v2`,** on `check` (default `v1`) and on the five
   use-case recipes (`submission = "v1"` in the judge's step, set with
   `--set judge.submission=v2`), on `whizbang squint check` (`--submission`), and in a matrix
   arm (`C;submission=v2`, or `submission = "v2"` in a table). It is in the decision cache's
   key, since every argument of `check` is, and in each run's recorded arguments. An arm
   names it in its identity only when it gives it, so every matrix written before keeps its
   identity. A matrix resumed on a binary whose default differs from the value a completed or
   failed cell of an arm that names none recorded is refused before anything runs
   (`whizbang::evaluation::submission_resumable`). `check` is 1.8.0.
2. **v1 is unchanged:** its schema, description, messages and one repair, byte for byte
   (`submission::the_judge_sees_the_same_messages_as_before`), and the baseline replay runs on
   it.
3. **v2 keys the items by checklist id.** The tool (`submit_verdict_v2.json`, written per job
   by `use_case::submit_tool_v2`) gives `items` as an object whose required keys are the
   job's checklist ids, `additionalProperties` false, each value v1's item answer without
   `id`; `findings`, `opinion` and the definitions are v1's. The adapters send it as they send
   v1: as written to Anthropic, not strict to OpenAI, its definitions written in place for
   Google.
4. **v2 reads near-misses that have one reading, and records each** in `rejections.json`
   under `recoveries`, `{turn, kind}`: `list_as_text` (a list, or the keyed object, given as a
   string that holds it), `array_form` (v1's list of answers, each with its id), `enum_word`
   (a disposition, confidence or location kind differing in case, surrounding space, or a
   space or hyphen for an underscore; `can't tell` and `cannot_tell` for `cant_tell`),
   `id_case` (a checklist id differing only in case or surrounding space, exactly one id
   matching), `identical_duplicate` (a second answer of an item the same in disposition,
   confidence, location and `allowed_by`, read as the first), `evidence_dropped` (an id of a
   call that did not complete, or evidence that is null) and `wrapper` (the submission inside
   an object whose only key is `submission` or `verdict`, once). Recoveries are recorded for a
   submission that was accepted. A differing duplicate, an id that matches none or more than
   one, and a violation without a location are never recovered. A recovered answer is
   accepted under v2; nothing claims it is the verdict a v1 repair would have reached.
5. **v2 names every problem in its one repair.** What is left after the recoveries is held to
   v1's rules unchanged (`decided_of`'s checks, in every-problem mode): the first problem of
   each item and each finding, then the items missing or given twice, each in v1's words,
   naming only ids the judge was given. At most 12 are named, then `and N more`. The
   rejection is recorded by its first problem's class, with the classes of the others under
   `also`. Still one repair, and the repaired submission is a whole submission.
6. **The second look keeps its contract.** v2 applies to the first submission and its repair.
   The first submission is turned into v1's array form before the second look reads it, and
   the second look's turn offers v1's tool, so `tentative_items`, `merged` and the strict rule
   see what they see under v1.
7. **v2 is a setting no default uses.** Task F2 runs v1 against v2 on the small judges and
   decides by the rule `submission/1`, frozen in the plan; if v2 is adopted, its default
   changes there, and this record is amended with the evidence.

## Consequences

- A run's arguments now hold `submission`, so every key of the decision cache made by 1.7.0
  misses, as any version change of `check` makes it do.
- `rejections.json` gains `submission` and `recoveries` beside T1's fields, and an `also` on
  a rejection of v2 that named more than one problem.
- The bundle is measured together: all problems at once, the recoveries and the keyed schema.
  The recoveries are counted by kind, so the record shows which were used; separating the
  schema's effect is a later slice's.
- A provider's strict mode may refuse the keyed form or its property names, which hold `:`
  and `@`; each adapter has a test of the form it sends.
- Reverting is a revert of the task's merge: no default uses v2 before F2.

## Alternatives

- **A partial repair** (asking again for the failing items only): it cannot reuse `merged`,
  and a repaired submission would no longer be whole. Dropped in the plan's first review.
- **Building v2 only after the judge round reads the classes:** puts the format behind the
  judge and the slice in series for a smaller change; the bundle is stated as a risk instead.
- **Recovering everything plausible** (an ambiguous id, a differing duplicate): a reading that
  is one of two is a guess at the verdict; refused.
