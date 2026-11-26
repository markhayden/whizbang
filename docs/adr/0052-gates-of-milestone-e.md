# 0052. Gates of milestone E

Status: accepted.
Date: 2026-11-25
Spec: section 8, "Gates", "Marks" and "Evaluation data"; section 9 (a failed attempt is
reported, never replaced by a later success); section 6 (a limit is enforced, an estimate is
advice); plan mE, task M1

## Context

Milestone E's squint rounds (plan mE, tasks R3, F2 and R4) run several gates on one matrix,
under five judges whose cells differ, on a fresh set that is held out until its one round.
Before E a matrix ran every set, item, judge, arm and repetition together; a gate named its
matrix by identity and ran exactly its two arms; arm C refused the second look; `eval
judges` knew only `judge-pick/1` and `submission/1`, at 2,000 resamples and 95%; a refusal
named paths, item ids and a parser's text; nothing listed what a gate admitted; and a
resumed matrix kept a failed cell failed without a rule written down for it.

## Decision

1. **A matrix runs only the cells it names.** A judge may list its arms, each with its own
   repetitions; a set's table (`[matrix.set."<id>"]`) may list its arms and its judges, by
   backend. A cell outside a list is not forecast, run, resumed or counted complete. A
   matrix without lists has the identity it had.
2. **Arm C takes the second look.** It is one more turn after a tentative item, not a
   detail measured first; details stay arm F's.
3. **Gate files of version 3 are frozen in their matrix.** The gate names its matrix by file
   alone; the matrix names each gate file by the SHA-256 of its bytes under `[gates]`, in
   its identity, which every run holds. `eval matrix` runs the cells and refuses a matrix
   whose gates do not hold; `eval run --gate` runs nothing and scores a gate from the
   matrix's runs, its two arms' cells (or its arm on its two sets) under the judges that run
   both. Version 2 files read, run and score as before.
4. **A gate across two versions of a set** names `arm`, `control_set` and `treatment_set`:
   two sets of one use case whose items and families are the same by id, which `eval
   matrix` checks before any cell runs. An item whose label differs between them is counted
   (`relabelled`) and scored under neither version's truth.
5. **One correction for the round.** Every gate on a matrix corrects for the same count of
   hypotheses: its gates' comparisons, one for each judge a gate compares, or, where the
   matrix states `hypotheses`, that count, which leaves room for the judge's rule's beside
   them, one for each judge in each scope it picks over. A round states its final count up
   front, before its first cell: R4 states 27 if tier 2 may run, 21 otherwise, and a count
   is never raised once a cell has run, since the count is in the matrix's identity. A
   matrix that states a count resamples at
   `resamples_for(count)`, the larger of 20,000 and 20 × 2 × H ÷ 0.05 rounded up to a
   thousand, so each tail holds at least 20 draws; every gate of the round states it.
6. **`judge-pick/2`** is `judge-pick/1`'s clauses at the level the matrix's correction gives,
   at that count of resamples, inconclusive below 20 families or with a tail the resamples
   cannot resolve; `eval judges --set <id>` scores the sets named alone. **`submission/2`**
   is `submission/1` over the items each judge scored under both arms, clause (a) pooled,
   (b) and (c) per judge, (c) read on the family-resampled 95% interval at its bound toward
   harm.
7. **A catch-rate gate.** `[measure] primary = "catch_rate"` gates on the gain in the share
   of expected failures caught, resampled over the families with at least one, with the
   repetitions combined by mean when the file says `combine = "mean"`; the guardrails are
   read as the file states them. It is the study's measure, for the repeat of its one
   comparison that excluded zero.
8. **A tool file's method is frozen in an arm** (`C+regions@eomt`, or `{ tool = "regions",
   method = "eomt" }` in a table), in the arm's identity. `check` is given it as `methods`,
   gives it to every call of that tool, and refuses a judge's call that names another. A
   tool file must declare a `method` argument for an arm to name one: `regions` gains its
   methods with plan mE's task T2, and `C+regions@eomt` runs only after it.
9. **A matrix may run past its forecast** (`eval matrix --past-forecast`): the forecast is
   printed and the matrix runs, each run admitted against the allowance; the first run the
   allowance refuses ends the matrix, the cells left named by count, and a run after a new
   allowance resumes it. Without the flag, a forecast past what is left is refused as
   before.
10. **Refusals tell nothing of a held-out set.** Under `--held-out`, every refusal of `eval
    sets check`, `eval sets fetch`, `eval matrix`, `eval run --gate` and `eval judges` names
    only the set's id, an item's place in the set and the kind of problem. Anything else is
    withheld but a refusal of a form known to tell nothing (about the allowance, the
    forecast or cells left, on one line, with no path, quote or id of an item): what passes
    is allowed, not what is caught.
11. **What was admitted is read from the receipts.** A gate file of version 3 may state
    `[admits]` (its rung, and the tool, use case, format, wording or judge on trial); its
    receipt holds it, its sets' splits and the day its last cell ended. `whizbang eval
    admitted` lists what a receipt admitted: an admitted outcome of a rung 4 gate whose sets
    are all `blind` or `heldout` and none exposed. A sealed set's split cannot be edited, so
    the exposed sets are a committed list (`whizbang_eval::gate::EXPOSED`): `blind-v3`, of
    split `blind`, and `blind-v1` and `blind-v2`, of split `heldout`, each used before. A
    held-out set joins the list after its one round (R4 adds `confirm-v1` and its twin). A
    rung 4 gate on an exposed set is refused when its matrix is read, and its receipt admits
    nothing; so does a rung 3 gate, or one on a dev set. Each record that states its tool
    admitted names the receipt that admitted it.
12. **No retry of a failed cell.** A resumed matrix or gate never runs a failed cell again,
    by section 9: a failure counts as an error, which the 2% guardrail sees. Retrying when a
    failure follows the item would bias the measurement toward the items a judge can do.

## Consequences

- R4 runs its cells once, in one matrix, under one correction, and each gate reads its own;
  the fresh set is read only through `--held-out`.
- A gate's file must be written before its matrix, whose identity then changes with any gate
  added: adding a gate after the first cell is a new matrix, with no runs.
- Receipts of version 2 carry a date and splits; those of version 1 admit nothing.
- `judge-pick/2` costs more resampling time than `judge-pick/1`: 20,000 resamples and more.
- To reverse: revert the merges. Gate files of version 3, matrices with lists and the new
  rules are refused again; receipts already written stay as records.

## Alternatives

- **A gate that names the matrix's identity, as version 2 does, beside a matrix that names
  the gate**: circular, since each would hash the other.
- **A gate per matrix, as before**: every treatment pays again for its own control, and the
  round's hypotheses cannot be corrected together.
- **Retrying a failed cell once**: a failure may follow the item, and replacing it with a
  later success is what section 9 forbids.
- **Admissions written by hand into each record**: what admitted a tool would not be held to
  the receipt that says so.

## Amended with task M2

The edits' gates (`SPEC.md` section 8: an edit tool's gate is task success, preservation
outside the mask and blinded quality) are not `eval run --gate`, which gates a judge's
balanced accuracy.

1. **`edit-gate/1`**, frozen before its round, pins its cases (`edit-cases/1`: each case's
   family and the request id of its run) and the lab protocol of its pairs by digest, and
   states its rung, the round's hypotheses, its level and seed, its minimum of families,
   and a bar for each measure: task success (a share of every case, from the checks of the
   finished edits on rung 3, from a person's marks on rung 4 and only there); preservation
   outside the kept mask, exact; the outcomes the decode of the pairs must reach and the
   pairs that must be picked; operational errors; cost (the mean settled cost of a run
   against the recipe's guide figure times a multiple); and latency of the local work alone
   (a run's time less its backend requests', over the record's time at the nearest size,
   the median against a multiple).
2. **Task success's lower bound is the lower of two**, each at the level corrected across
   the round's recipes (Bonferroni): the percentile of a bootstrap that resamples families
   at `resamples_for(count)`, and the Wilson bound over the cases. A round of 8 cases in 4
   families gives a bootstrap a handful of families to draw; Wilson's bound does not lean
   on their spread. A run that failed, or is missing, is an error and not a success, never
   replaced by a later one; a finished edit with no check or mark leaves the gate
   inconclusive.
3. **The outcome**: rejected when preservation, errors, cost, latency or quality fails;
   inconclusive when the data are short, whatever task success read; rejected when task
   success misses a bar; else admitted. `whizbang eval edits gate` writes the receipt,
   `edit-receipt/1`, aggregates only, and `eval admitted` lists its recipe when it was
   admitted on rung 4 from marks.
4. **Marks and pairs of any recipe's runs**: `eval loop sheet --cases` and `lab pairs
   --from-runs` read each case's run from the store by its request id, a recipe's or the
   loop's, so no round has to run through the lab to be marked or picked. The owner's marks,
   picks and assignments are refused inside a git work tree.
