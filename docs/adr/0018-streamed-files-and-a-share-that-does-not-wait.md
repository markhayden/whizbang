# 0018. Streamed files and a share that does not wait, on the store's file-system interface

Status: accepted. Names ADR 0017 ("Holds": the try at a share that it says the store's
interface gains with task M5 is here, before that task; nothing of ADR 0017 is changed)
Date: 2026-11-14
Amended: 2026-11-14, after the review of its pull request. Changed: what a crash
leaves of the bytes that were not synced is a test's choice of four (`Tail`); a
streamed file that was removed or replaced and that a crash brings back is cut as any
other, which the first form restored whole. Added under "Limits": how the try opens
its file. Words: rule 2 says who removes a file that is not whole, and which tries
are bounded by a thread and a timeout.
Spec: section 3, "Storage"; section 4, "Segmenters"; section 6, "Controls from table-manners"

## Context

The store reaches the disk through one interface, `Fs`
(`crates/whizbang-core/src/store/fs.rs`), so that a test can stand between it and the
disk (`ShimFs`, `crates/whizbang-testkit/src/fs_shim.rs`): refuse a chosen call, and
lose at a crash what was never made durable. The models directory is built on the same
interface (ADR 0017), and needs two things of it that the store never did.

- **A file that is not held in memory.** `Fs::write_new` takes a file whole, as bytes,
  and `Fs::read_at_most` hands one back whole. A file of a model is up to 1 TiB
  (ADR 0017, `FILE_MAX_BYTES`), arrives piece by piece from a download, and is hashed
  back before it is published.
- **A shared lock that is tried.** `Fs::share` waits for one who has the file alone. A
  hold on an installation is taken under the directory's lock, whose holder waits for
  nothing (ADR 0017, "Three kinds of lock, and their order").

What there was to build on: `write_new` creates with `O_EXCL`, which fails on a name
that is taken and follows no link there, and syncs through the handle that wrote; a
name is made durable by `Fs::sync_dir`, and a file is brought to its place by
`Fs::rename`, both calls of their own; the locks are the standard library's, `flock`
on Linux and macOS; the workspace forbids `unsafe` and has no crate that names a
system's open flags.

## Decision

Three calls are added to `Fs`, with two kinds of handle. The code is in
`store/fs.rs`; `RealFs` and `ShimFs` give all three. The tests named `fs::…` are in
`crates/whizbang-core/tests/it/fs.rs` and run each on both file systems; those named
`streams::…` are in `crates/whizbang-testkit/tests/it/streams.rs`; those named
`store::fs::tests::…` are beside the code.

### A new file, written piece by piece

`Fs::begin_new(path, most)` creates an empty file and answers a `NewFile`:

- **`append(piece)`** adds a piece to the end. A piece holds `PIECE_MAX_BYTES` (1 MiB)
  at most. The file never grows past `most`, which the caller states when it opens the
  file: a piece that would take it further is refused with `FileTooLarge` before any
  of it is written (`fs::a_file_grows_to_the_most_its_opening_stated_and_no_further`,
  `fs::a_piece_over_the_bound_is_refused_before_anything_is_written`). The loop is the
  caller's; `most` is what bounds it.
- **`sync()`** makes durable every byte appended so far, through the handle that
  wrote them, as `write_new` does and for its reason.
- **`finish()`** closes the file and says its length. **It answers only for a file
  whose every byte was synced through that handle,** and writes nothing itself: with
  bytes appended since the last sync it fails
  (`fs::finish_answers_only_for_a_file_whose_every_byte_was_synced`).

**The name is taken as `write_new` takes one:** the call fails when a file, a
directory or a link has it, and follows no link there
(`fs::a_name_that_is_taken_is_refused_and_what_has_it_is_left_alone`,
`fs::a_streamed_file_follows_no_link_at_its_name`). The file is readable by its owner
only.

**A half-written file is never taken for a whole one, by three rules together.**

1. The file is under its name from the moment it is opened, with whatever was appended
   so far. So that name is one that nothing reads as a whole file: it is where work
   in progress is kept. The file is brought to its place by `Fs::rename`, and the
   name made durable by `Fs::sync_dir`, as the store publishes: there is no second
   way to put a file in place.
2. Only a file that `finish` answered for is whole. A handle that was dropped, or
   whose `finish` failed, leaves a file that is not. The file stays under its name;
   removing it is the caller's.
3. **A call that fails breaks the handle:** every call after it fails, `finish`
   among them, whatever failed and however little
   (`fs::a_handle_that_answered_with_an_error_answers_nothing_more`). A file that was
   whole and synced before a refused piece is not finished after it.

The rules that need no disk (the bound on a piece, the most, what was synced, what
broke) are one type, `Streamed`, which the real file system and the shim both count
with, so that the two cannot differ
(`fs::the_count_admits_a_piece_up_to_the_bound_and_a_file_up_to_its_most`,
`fs::the_count_is_whole_when_every_byte_is_durable_and_never_once_broken`).

### A file read piece by piece

`Fs::open_file(path)` answers an `OpenFile`, whose `read(buffer)` reads the next bytes
into the caller's buffer and says how many. **The buffer is filled unless the file
ends first,** so fewer than it holds means the end and none means nothing was left
(`fs::a_file_is_read_in_pieces_each_full_but_the_last`). A buffer that holds nothing,
which would read as the end, or more than `PIECE_MAX_BYTES`, is refused. A read that
fails breaks the handle, as above: it has moved on in the file.

**Only a regular file under its own name is opened.** A link, to a file, to a
directory or to nothing, a directory and anything else are refused with `InvalidData`
and not followed (`fs::only_a_regular_file_is_opened`,
`fs::a_link_is_not_followed_to_what_it_names`). `read_at_most` follows a link; this
does not. How it is done, without a flag for the opening: the name is looked at
without following a link and refused unless a regular file has it; then it is opened;
and the open file must be a regular file and the very one the look found, by its
device and number. So what is read is a file that had this name itself, as no link,
at one moment of the call. A link put there after the look is followed to another
file, which is not the one found, and is refused
(`store::fs::tests::a_name_that_leads_elsewhere_between_the_look_and_the_opening_is_refused`).

**How long the file may be is the caller's to hold.** It knows the size it expects,
counts what it reads, and stops one byte past it, as a caller of `read_at_most` asks
for one byte more.

### A share that does not wait

`Fs::try_share(path)` is `Fs::share` without the wait: it answers a share, or `None`
when one has the file alone, and returns at once either way. It writes and creates
nothing, and fails with `NotFound` when there is no file. It is `flock` with
`LOCK_SH | LOCK_NB` through the standard library, on Linux and macOS, on a file
opened for it, as `share` is. So everything `store/fs.rs` says of these locks holds
for it: local file systems only, the lock is on the file and not on its name, it is
advisory, and it is let go, not only closed, when its value is dropped
(`store::fs::tests::a_share_given_by_a_try_and_let_go_is_gone_though_a_copy_of_the_open_file_is_still_held`).

**What a process's own locks do to its own tries.** Each lock here is on a file opened
for that lock alone, and `flock` belongs to the open file, not to the process. So a
process's locks count against its own tries exactly as another process's do:

| The file is held | By the process that tries | By another |
|---|---|---|
| By nobody | given | given |
| Shared, by `share` or by `try_share`, once or more | given: a second share, which is a second lock | given |
| Alone, by `try_alone` | refused | refused |
| As a lock file, by `hold_lock_file` | refused | refused |

And a share that a try was given stops `try_alone` and `hold_lock_file`, in its own
process and in any other, until it is dropped
(`fs::a_try_at_a_share_is_given_beside_shares_and_refused_while_one_has_the_file_alone`
on both file systems; every cell, with the shim standing for one process and for two,
`streams::a_try_at_a_share_is_given_or_refused_at_once_whoever_holds_the_file`). In
those two tests no try waits on a holder: each try that may be refused is made on a
thread and waited for ten seconds at most, so a try that waited fails the test and
does not hang it. The unit test beside the code tries only a file that nobody has
alone.

### A file system that does not give them

The three calls have a default in the interface that fails with `Unsupported` and
touches nothing, so a file system written before them (the wrappers in the store's
tests) is not changed by them. The default of `try_share` is an error, never `None`:
`None` says that somebody has the file alone
(`fs::a_file_system_without_the_new_calls_says_so_and_touches_nothing`).

### What an error says

No error of these calls names a path: one from the system is the system's, which
holds none, and the interface's own are fixed words. The kinds a caller may decide
by: `AlreadyExists` and `NotFound` from the opening calls, `FileTooLarge` past the
most, `InvalidInput` for a piece or a buffer out of bounds and for a `finish` with
bytes not synced, `InvalidData` for a name that is not a regular file's own,
`Unsupported` from the default.

### The shim

**Every step is an event** that can be refused or be where the crash falls: `Begin`,
`Append`, `SyncFile` (the event `write_new`'s sync already is), `Finish`, `Open`,
`Read`, and `TryShare` for a share that was given. A try that found the file held
alone is no event, as a refused `hold_lock_file` is none. The store's own reads and
locks are still no events; a piece of many must be refusable, and so must a try, so
these are (`streams::each_step_of_a_streamed_file_is_an_event_and_so_is_each_piece_read`,
`streams::a_refusal_of_any_step_of_a_streamed_file_fails_it_there_and_drops_nothing`,
`streams::opening_and_each_read_can_be_refused_and_a_refused_read_ends_the_reading`,
`streams::a_try_at_a_share_can_be_refused_as_a_step_and_then_holds_nothing`). What the
interface refuses itself (a piece too long, a buffer that holds nothing) is no event.
A refused step breaks its handle, as an error of the real disk does
(`streams::a_handle_whose_step_was_refused_answers_nothing_more`).

**What a crash leaves of a streamed file is one outcome for each choice a test
makes, the same every time.** If the file's name survives, which `Lost` decides as for
any new file, every byte that a sync made durable is there, and what follows is the
test's choice (`Tail`, set by `ShimFs::with_tail` or `ShimFs::set_tail`):

| `Tail` | What follows the synced bytes |
|---|---|
| `Half`, the default | The first half, rounded down, of the bytes appended since the last sync, and nothing after |
| `Zeroed` | The file has its full length, and every byte appended since the last sync is zero |
| `All` | All of the bytes appended since the last sync |
| `None` | Nothing: the file ends where its last sync left it |

Each choice gives exactly that, for a file that stays, for one that was renamed, and
for one that was removed or replaced and that the crash brings back
(`streams::each_choice_of_tail_leaves_exactly_what_it_says_of_a_file_that_stays`,
`streams::each_choice_of_tail_leaves_exactly_what_it_says_of_a_file_that_was_renamed`,
`streams::each_choice_of_tail_leaves_exactly_what_it_says_of_a_file_that_was_removed_and_comes_back`).
**A streamed file that a crash brings back is cut as any other:** a removal or a
replacing rename that the crash undoes restores what the crash would have left of the
file, not all that was appended to it, whether the file was removed by itself or with
its folder (`streams::a_streamed_file_that_a_crash_brings_back_is_cut_as_any_other`);
and a file renamed onto itself is still cut
(`streams::a_streamed_file_renamed_onto_itself_is_still_cut_at_a_crash`). The choice
is of streamed files only: a file whose `write_new` was interrupted is cut to half,
as before.

Under the default, a file whose every byte was synced is whole, finished or not
(`streams::a_file_streamed_and_synced_is_whole_after_a_crash_finished_or_not`); a
file never synced is the first half of itself, as a file whose `write_new` was
interrupted is, and one byte not synced is lost
(`streams::a_file_streamed_and_not_synced_is_gone_or_cut_after_a_crash`); a reader
after the crash is given all that was synced, never a byte that was not appended, and
never all of what was not synced
(`streams::after_a_crash_a_reader_is_given_what_was_synced_and_at_most_half_of_the_rest`).
The crash is tried in place of each of the seven events of a file streamed in two
synced pieces, under each way of losing its name
(`streams::a_crash_at_any_event_of_a_streamed_file_keeps_what_was_synced_and_half_of_the_rest`,
in the nextest group of the crash sweeps). The file is cut under the name it has at
the crash, renamed or in a folder that was, and a name used again is not cut for the
file that had it before
(`streams::bytes_not_synced_are_cut_under_the_name_the_file_has_at_the_crash`,
`streams::a_name_used_again_is_not_cut_for_the_file_that_had_it_before`). After the
crash a handle fails and writes nothing, and a share a try was given is gone
(`streams::after_a_crash_no_handle_reaches_the_disk`,
`streams::a_crash_ends_what_a_try_was_refused_for_and_the_share_a_try_was_given`).

### Limits, each stated

- **The shim's outcomes are four of many.** A real disk may keep all of what was never
  synced, or none of it, or any prefix, or a file of the full length whose unsynced
  part is zeros or other bytes. The shim shows half by default and, by a test's
  choice, all, none, or the full length with the unsynced part zeroed; it does not
  show other bytes there. A caller that decides by a file's length after a crash is
  exposed only under the zeroed choice.
- **The try opens its file as `share` does:** a link is followed and a pipe at the
  name would make the opening wait. A lock file is made by whizbang and never
  replaced (ADR 0017).
- **The cut stands for a crash inside an append.** As an event an append happens whole
  or not at all; the cut falls where half of the unsynced bytes ends, inside a piece
  or between two. A refused append leaves nothing of its piece in the shim; on a real
  disk a failed append may leave any part, and the broken handle is what keeps that
  file from being finished.
- **A failure of a real append or sync cannot be made in a test,** so that a real
  failure breaks the real handle is read in the code and held by test only in the
  shim.
- **A pipe at the name.** The opening looks before it opens because opening a pipe
  waits for its writer. A pipe put under the name between the look and the opening
  makes the opening wait: only one who can write the folder can do it. Closing that
  needs the flags of the system's `open` (no follow, no wait), which the standard
  library does not name and the workspace has no crate for
  (`docs/carry-over.md`).
- **One file taken for another.** The opening tells two files apart by device and
  number. A file that was deleted after the look, whose number the system then gave
  to a new file, with a link to that file put under the name, all between the look
  and the opening, would be read. It needs the same writer in the folder.
- **Elsewhere than Linux and macOS** two files cannot be told apart this way, and the
  opening holds only that the name was a regular file at the look and leads to one.
- **A read does not lock.** A file that is written while it is read is read as it
  changes. What must not change while it is hashed is kept by the caller's lock.

## Consequences

- The installer (tasks M2, M2b) writes a download through `begin_new` under a name in
  staging, with the file's size as `most`, syncs, finishes, and hashes it back through
  `open_file`, holding one piece at a time. A hold (task M5) is `try_share` on the
  hold file under the directory's lock; whoever deletes an installation goes on using
  `try_alone`.
- The store uses none of the three, and nothing it did has changed: `write_new` shares
  the opening of a new file with `begin_new` and is otherwise as it was, and the
  store's tests pass as they were.
- A wrapper around a file system that does not pass the three calls on answers
  `Unsupported` for them, loudly, and not the answers of what it wraps.
- `Event` has six more variants. A test that crashes before every event of something
  that streams or reads piece by piece has an event for each piece.
- Reversing this costs the callers their loops and nothing on a disk: no format is
  involved.

## Alternatives

- **Write under a temporary name inside the call, and rename at `finish`.** Turned
  down: the store already publishes by `rename` and `sync_dir`, as calls a test can
  refuse one by one, and an installation's files are staged as a folder that is
  renamed whole (ADR 0017, "Publishing"). A rename hidden in `finish` would be a
  second way, and a step no test could refuse apart.
- **`finish` that syncs.** Turned down: the sync and the finish would be one call
  and two events, and a caller could not tell which failed; and "whole" would no
  longer mean "synced through this handle by a call that said so".
- **Let a handle go on after a refused piece.** Turned down: a download longer than
  its file's size would be refused its last piece and could then be finished as a
  file of the right length that is the start of something else.
- **Required methods, no defaults.** Turned down: the five wrappers in the store's
  and the executor's tests would each be edited to pass on three calls they never
  see, and the store's tests were to pass as they were.
- **A second interface for the models directory.** Turned down: its locks and its
  files are on one disk with one crash, and ADR 0017 builds on the one interface.
- **A record lock of `fcntl` for the try,** which a process's own shares would not
  refuse. Turned down for the reasons `store/fs.rs` gives for the store's locks, and
  because two holders in one process must exclude each other: the tests of the
  models directory, as of the store, stand a shim for a process.
- **Refuse a link by the flags of `open`.** Turned down for now: it needs a crate
  for the constants, which differ between Linux's architectures, or `unsafe`. The
  look and the comparison refuse every link without either.
- **One outcome for the unsynced bytes, half,** as `write_new`'s unsynced file has.
  It was the first form of this record and was turned down in review: an installer's
  crash sweeps must be held to a file's hash and not to its length, and only a file
  of the full length with its unsynced part zeroed shows the difference.
- **The choice of tail inside `Lost`.** Turned down: `Lost` is given at every crash
  in the store's tests, which would each be edited, and the tail is of content while
  `Lost` is of names.
- **The length of an open file, from its handle.** Turned down: the caller counts
  what it reads and stops one byte past what it expects, which bounds the reading
  whatever the file says of itself.
