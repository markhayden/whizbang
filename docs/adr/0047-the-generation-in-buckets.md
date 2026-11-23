# 0047. The generation in buckets

Status: accepted.
Date: 2026-11-23
Spec: section 3, "Storage" (unchanged); the store's own description
(`crates/whizbang-core/src/store/mod.rs`, "On disk", "Buckets", "Publication" and "Prune");
ADR 0030; ADR 0036; plan m6, task S1, item 1

## Context

A generation file held every name of the store, and every publication wrote it whole (ADR
0030). Its size grows with every run, so a publication is O(runs), and a store that a server
holds for months reaches the 64 MiB limit of a generation file: at about 39,000 runs of the
kind the playground leaves (the carry-over row on every publication writing the whole
generation, measured by plan m5, task H1: 1,706 bytes a run, 712 ms a publication at 10,000
runs). ADR 0036 took away the trail of older files; the writing of the whole file at each
publication stayed. `whizbang serve --http` (plan m6, task H1) holds one store for as long as
an app runs: thousands of runs a day.

## Decision

1. **Every name of a namespace that grows with use lives in one of 256 buckets.** A name's
   bucket is the first byte of the SHA-256 of its key:
   - a run's names: the run's id;
   - `requests/<id>/...`: the request id;
   - `spend/<form>/...` for a form that names a run (`reservations`, `held`, `place`,
     `dispatched`, `settling`, `settled`, `closed`, `adjusted`): that run's id, so a paid
     attempt's names share a bucket with its run; any other name under `spend/` (a turn, a
     reset, an epoch's guard): the name itself;
   - `imported/<name>`: the key of `<name>`, so a mark shares the bucket of what it marks;
   - any other name with a `/` in it (an artifact's name, the playground's names of a
     picture): the name itself.

   A name with no `/` is one of the store's few fixed names and stays in the generation
   file. The rule is the store's alone, in one place (`store::buckets`); a name in a bucket
   that the rule does not put there is damage. The rule is fixed by the format: a change to
   it is a new format.
2. **The generation file keeps the fixed names and 256 digests.** Format 5: `format`,
   `number`, the fixed names, and `buckets`, one string of 256 digests of 64 hex digits,
   one for each bucket in order: the digest of the pack that holds that bucket, or 64 zeros
   for an empty bucket, which is held nowhere. So the generation file no longer grows with the store: it is about 16.5 KB, and its
   size changes only with the digits of its number and with the fixed names.
3. **A publication writes the buckets it changed in one file, a pack.** A pack is a file
   beside the generation files, `generations/<sha-256>.pack`, named by the SHA-256 of its
   bytes: `format` and `buckets`, each bucket it holds under its index, with its names
   outside runs, its runs, and what the marks of its imports say (when each import was made,
   and the names that came in with it, for the store's collection of unused imports, plan
   m6, task S1, item 7). It lives beside the generation files and not under `objects/` so
   that the sync of `generations/` that makes the generation file durable makes the pack
   durable too: a publication syncs no folder more than it did before buckets. What a publication changes is known from what it does (the buckets of the
   runs it makes, adds to, closes or deletes, of the names it puts, makes, keeps and marks,
   and of the names a deletion or a collection lets go); nothing is compared. A publication
   that changes no bucket writes no pack. A pack takes buckets in order while it is under 16
   MiB (`PACK_BYTES`), so a publication that changes many large buckets (the first onto a
   large store of format 4, or one batch that makes thousands of runs) writes several; one
   that changes a few, which is every publication of a run, writes one. The plan asked for one file per bucket; a pack is
   one file per publication instead, for two reasons met in building it. The number of files
   a publication writes would then depend on whether the keys it changed share a bucket,
   which a run's id, made of the time and a count, decides: the store's crash sweeps, which
   learn the steps of a publication once and then crash before each, met a different number
   of steps from one run to the next. And each file is a name that is not durable until its
   folder is synced, so the names a crash can keep or lose, which those sweeps try two to the
   power of, grew with every bucket. One pack keeps both as they were: one name, one place.
4. **A pack is not kept for a few of its buckets.** A bucket a publication did not change is
   carried into its pack too when fewer than half of the buckets of its own pack are still
   named: so every pack the current generation names holds at least half live buckets, and
   what packs keep is at most about twice the buckets the generation names. A bucket is
   carried at most once a pack, so this adds at most a constant factor to what is written.
5. **A pack is written in place, and before the generation file.** It is written under its
   digest and synced before the generation file that names it is renamed into place, and
   the one sync of `generations/` after that makes both durable (step 3 of a publication). It is not staged: no generation that became visible
   names a pack that is not whole and durable, so a file a crash cut short is named by
   nothing, and the next publication that needs that pack finds it not whole and writes it
   again. A pack that is there and whole is not written again.
6. **The present format is read as it is.** A generation file of format 4, the whole store
   in one file, is read as before and its names sorted into buckets in memory; the first
   publication by a writer of this build writes every bucket that holds anything in one pack
   and a generation file of format 5, once. A build before this one reads a format other
   than 4 as another format and refuses the store by its version, naming it
   (`Error::OtherFormat`), rather than misread it. A reader of this build reads formats 4 and
   5 and refuses any other the same way.
7. **A publication removes the packs it superseded.** After a publication's step 6 has
   removed the files of older generations that nobody holds (ADR 0036), the writer removes
   each pack it has superseded, in this publication or an earlier one, that neither the
   current generation, nor a generation file still there (one a reader or a hold shares),
   nor a publication in progress names. So the store keeps the packs of the current
   generation and of the held ones, and no others, without a prune. A pack a crash left
   (written, and named by no generation that became visible) is content that nothing names,
   and goes at the next prune, which removes every pack that no generation it keeps names
   and reports how many (`Pruned::buckets_removed`).
8. **A pack is read once.** A pack is immutable and named by its digest, so the packs of
   the last generation a writer or a reader read are kept in memory by digest, and a
   snapshot reads from disk only the packs it has not met. The in-memory generation holds
   its buckets shared, and a publication copies only those it changes.

## Consequences

- A publication writes the generation file, about 16.5 KB, and one pack of the buckets it
  changed: for one run of the engine API, the run's bucket (with its ledger's names), its
  request id's, its upload's and one for each turn of the ledger it took. Measured by hand at
  10,000 and 100,000 runs as the engine API leaves them
  (`store::volume::a_hundred_thousand_runs_publish_within_the_measured_bound`); the numbers
  are in the carry-over row on every publication writing the whole generation.
- The 64 MiB limit is now a limit on one pack and on the generation file each, not on the
  store: a publication that would write either larger is refused before anything of it is
  visible, as before. The seam by which a test lowers it (`Store::limit_generations_to`)
  holds instead what the generation's names take written, its file and each bucket
  (`Generation::names_bytes`), so that a test brings a store to its limit by its names as it
  did before buckets.
- **Reverting is costly.** A store a writer of this build has published is in format 5,
  which a build before it refuses. A revert keeps this reader and writer of buckets and drops
  the rest; or a store is exported with this build and imported into a new one.
- Recovery, prune and ADR 0036's removal of older generations hold for packs as for any
  other object: a crash at any step of the first publication onto a store of format 4 leaves
  one whole generation (`store::buckets::a_crash_at_any_event_of_the_first_bucket_publication_leaves_one_whole_generation`).
- A test that changes a generation by hand writes it again as format 4 first
  (`whizbang_testkit::stored::whole`), since a pack is named by its digest and a change to
  it is damage.
