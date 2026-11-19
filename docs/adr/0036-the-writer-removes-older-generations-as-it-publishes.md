# 0036. The writer removes older generations as it publishes

Status: accepted.
Date: 2026-11-19
Spec: section 3, "Storage" (unchanged); the store's own description
(`crates/whizbang-core/src/store/mod.rs`, "Prune" and "Publication"); ADR 0030

## Context

A store kept the file of every generation it ever published until someone ran
`whizbang store prune`, and prune needs the writer lock, so it cannot run beside the process
that is writing. A generation file is the whole store's names, written again at every
publication (ADR 0030), so the trail grows with the square of the work done.

Measured on the owner's Mac, in use:

- An evaluation matrix of 528 decisions left a store of 49 GB: 13,305 generation files of
  about 5.9 MB each, and 660 MB of content. `store prune` took 3 seconds and left 665 MB.
- A running gate's store reached 16 GB a sixth of the way through its run.
- The machine's disk filled twice.

### Why a generation is about 6 MB

A generation file holds every name of the store: each run's record, journal, outputs and
diagnostics, and the records outside runs, each as a digest, a length and a kind, in
indented JSON, about 200 bytes a name. Plan m3's task R1 measured 585 bytes a run for runs
with few names (`store::volume`). 13,305 publications, at about two a run (its admission and
its end), is some 6,600 runs; 5.9 MB over that many is about 900 bytes a run. That is an
estimate from the counts above, not a measurement of that store's names.

So the size grows with every run, and every publication writes it whole: publishing is
O(runs) each time. Plan 2C's volume test measured 2,058 ms for one publication at 100,000
request ids, a 38.9 MB file, against 61 ms at 2,000 (ADR 0030). That cost is a defect of its
own, and it stays after this decision: the trail goes, the writing of the whole file at
each publication does not. It is the existing row on the carry-over list on every
publication writing the whole generation, which this decision updates with the matrix's
figures.

## Decision

1. **A publication's last step removes older generation files that nobody holds.** After
   the root is synced (step 5, durability), the writer lists `generations/` and removes the
   file of every generation numbered below the one it just published that it can take
   alone: prune's own rule and prune's own lock (`Fs::try_alone`), applied by the code
   prune now shares (`Files::remove_older_generations`). It runs under the writer lock and
   with the step that one publication of a process runs at a time held, as prune does. So
   the bound is the newest one, N = 1, plus each generation a reader or a hold shares now.
   A generation let go goes with the next publication.
2. **A reader keeps what it holds.** A file that a reader or a writer's hold shares is not
   taken alone and is not removed, by this step or by prune: the guarantee is unchanged.
3. **Content stays prune's.** The step removes generation files only. Content that only
   older generations named stays until `store prune`, as before. It is the small part:
   one file a change, once, against one whole generation a publication.
4. **Nothing of it can fail the publication.** The publication is visible and durable
   before the step starts; an error in it (a file it cannot list, lock or remove) is
   dropped, and what was not removed is removed by the next publication or by prune.
   `Published` is unchanged.
5. **No sync of the directory.** A removal that a crash undoes leaves an older generation
   file, complete and named by nothing current, which the next publication or prune
   removes. The next publication's step 3 syncs `generations/` anyway.
6. **The writer's own snapshot reads the pointer again.** `Store::snapshot` holds nothing,
   so the file of the generation it found can now go between its reading of the pointer
   and its reading of the file, when another thread of the process publishes. It reads
   the pointer again when that happens and the pointer has moved, as a reader's snapshot
   already did; a missing file under a pointer that has not moved is still damage.

`store prune` still works and is idempotent: it removes what the writer could not (a
generation held at the time and let go since, a file a crash brought back, the trail of a
store written before this change) and content, and a second run removes nothing. `store
delete` reports the prune it runs, whose count of generations removed is now mostly 0: the
deletion's own publication removed the one before.

Held by tests (`crates/whizbang-core/tests/it/store/growth.rs`):
`a_writer_that_publishes_200_times_leaves_the_current_file_and_each_held_one_until_let_go`
(the bound after each of 200 publications, a reader's and a hold's generation kept and read
until let go, then prune finds nothing to remove, twice);
`a_crash_at_any_event_of_the_removal_after_a_publication_leaves_the_store_whole` (a crash
before each event from the final sync on, under every loss the test kit's disk can bring:
the pointer names a whole file, the held generation is whole, the next writer finishes the
removal, and prune finds no generation left);
`a_writers_snapshot_whose_file_its_own_publication_removed_reads_the_current_one`. The
crash sweeps that cover every event of a publication already reach the new step
(`store::a_crash_at_any_event_leaves_one_whole_generation_and_a_store_that_still_publishes`,
`commit::a_run_crashed_inside_its_commit_...`, `recover::a_crash_at_any_event_of_a_recovery_...`).

## Consequences

- A store holds one generation file, plus those held, at every moment a writer is not
  mid-publication: for the matrix above, its content and one file of about 6 MB, in place
  of 49 GB. After its prune that store was 665 MB, its content and one generation file,
  so nearly all of the 49 GB was generation files. The content that only older
  generations named still waits for `store prune`.
- A publication does a little more: a listing of `generations/` (one or two names) and a
  lock and a removal. It still writes the whole generation: the time and the bytes written
  of the measurement above are unchanged, and a matrix like the one above still writes
  some 49 GB over its run.
- A reader in another process that reads the pointer and then finds the file gone reads the
  pointer again: that was so before, and now happens at a publication, not only at a prune.
  The window is the moment between its two reads, against a publication's milliseconds.
- Older generations are no longer there to look back at between prunes. Nothing in
  whizbang read them but tests, which now hold what they look at.
- Reversing it is removing one line from `Publication::publish`.

## Alternatives

- **The long-running doors prune every K publications** (the matrix and gate runner,
  `serve`): turned down. It bounds only the doors that remember to, leaves every other
  writer (a command, a recipe, a test) with a trail, and also removes content in the middle
  of a run, which a parent reading what its child committed outside its hold can meet
  (carry-over, the row on what running an action does not do yet). The writer is the one
  place every publication passes.
- **Keeping the newest N > 1 files:** turned down. Nothing reads an older generation by
  its file but a holder, which keeps it anyway; a larger N multiplies the disk used by the
  size of a generation (38.9 MB a file at 100,000 ids) and leaves prune something to
  remove after every publication.
- **Generations as deltas** (each file the change, the current state rebuilt or compacted):
  turned down here. It is a change of the file format and of every reader, needs a
  compaction with its own crash story, and is the larger fix the carry-over row on the whole
  generation asks for: moving names out of the generation file. It would fix the cost per
  publication too; this decision fixes the trail only.
- **Syncing `generations/` after the removals:** turned down: one more sync a publication
  for no guarantee, since a removal undone by a crash is harmless.
