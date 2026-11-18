# 0030. Request ids and paid runs in volume

Status: accepted.
Date: 2026-11-18
Spec: section 3, "Storage"; plan m2c (`docs/plans/m2c-milestone-2.md`), task E1, item 6;
ADR 0007, ADR 0008, ADR 0016

## Context

Every publication writes the store's whole generation file again, and every request id adds
two names to it (`requests/<id>/admitted` and `/ended`) that are never removed (ADR 0007). A
paid run adds at least six names under `spend/` (ADR 0016), and the real ledger folded every
name under `spend/` and read the content of each at every reservation and every confirmation,
of epochs a reset had closed too. Milestone 2 is the first to run in volume: a matrix of 220
decisions, then the gate's. The carry-over list asked for both to be measured and decided
before the first matrix runs.

## Measurement

`store::volume` (`crates/whizbang-core/tests/it/store/volume.rs`) makes a store that holds the
names of N request ids in one publication, then times one publication of one more name, in
the tests' build on the owner's Mac (2026-11-18):

| Ids | Generation file | One more publication |
|---|---|---|
| 2,000 | 778,240 bytes | 61 ms |
| 100,000 | 38,900,240 bytes | 2,058 ms |

So an id costs 389 bytes of every generation file, and a publication about 21 ms for each
thousand ids the store holds. A run publishes three to five times (its admission, its
dispatches and settlements, its end). The generation file's limit of 64 MiB
(`GENERATION_MAX_BYTES`) is reached at about 170,000 ids.

## Decision

1. **The cost of request ids is accepted for milestone 2.** A matrix does not give its runs
   request ids: a cell's identity, attempt included, is in its run's arguments (`cell`), which
   is what a matrix resumes by. So the runs of milestone 2 add hundreds of ids at most, from
   people's commands, where a publication costs under a tenth of a second. The ids stay in the
   generation file and none is forgotten; `SPEC.md` section 3 is unchanged. The test at 2,000
   ids holds the measured bound in CI (`store::volume::request_ids_at_2000_…`), and the one at
   100,000 is run by hand when the store's publication changes.
2. **The ledger folds the books from the newest reset at an admission.** A reset closes the
   books (ADR 0016, "The strand cut down"): nothing made before it counts against the
   allowance it sets. So at a reservation and at its confirmation the ledger reads the content
   of a name only where it bears on the newest epoch, or on an epoch a hold of the run being
   confirmed names (`spend::fold::fold_newest`, `Reading::of_newest`): the reservations with a
   hold there, and that epoch's resets, guards and turns. Every other name is taken by its
   text alone. What the fold says of those epochs (the balance, whether the epoch needs its
   owner, the next turn, whether an admission stands) is what the whole fold says; what it
   says of an older epoch is not used. `whizbang spend` still folds everything.
   `spend::ledger::an_admission_after_many_paid_runs_and_a_reset_reads_no_closed_epoch` holds
   it: after twelve paid runs and a reset, an admission reads no file of epoch 1.

## Consequences

- A store that runs many thousands of runs pays for them at every publication: the names of
  runs grow the generation file as ids do. Moving names out of the generation file is not
  done; the measurement above is what a later milestone starts from.
- Within an epoch, the ledger still reads every reservation of that epoch at every admission;
  the owner's reset is what bounds it.

## Alternatives

- **Request ids for a matrix's cells,** so that a resumed matrix is answered by the executor:
  turned down; it would add an id for every attempt of every cell, the volume measured above,
  for what the cell argument already gives.
- **Forgetting ids older than an age:** a change to `SPEC.md` section 3, and not needed at
  this volume.
