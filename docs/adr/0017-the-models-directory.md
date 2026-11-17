# 0017. The models directory: a catalogue, an installation and its identity, a pointer, and holds

Status: accepted
Date: 2026-11-14
Amended: 2026-11-14, after the second review of its first pull request, with the second
pull request of task M1. Words only. Changed: the order of the locks gains the holder
of a hold file alone, and says that a lock taken alone is tried and only a shared one
waits; a hold's share of its hold file is a try; removal tries for the model's lock
until its deadline and sets its mark again; on the busy path the pointer is put back
before the mark is taken off. Added under "For later tasks": what tasks M2b, M3 and M5
to M6 must hold to, and the crashes their tests cover. Changed with the catalogue: the
parts it builds are marked as code and name their tests.
Amended: 2026-11-14, with the third pull request of task M1. Words only. Added: under
"Limits", which line a refusal names; a row of the crashes, for one after the pointer's
rename is durable; that what is printed of an entry's parts is counts. Changed with the
installation record: the parts it builds are marked as code and name their tests.
Amended: 2026-11-14, with task M2. Added: "Downloading into staging", which is code
(`directory.rs`, `stage.rs`, and the test kit's fake of a download), on the streamed
files of ADR 0018; under "For later tasks", how the pointer and a record are read back
and what is damage; under "Limits", a link where a fixed folder belongs.
Amended: 2026-11-14, with the first pull request of task M2b. Added: "The status
record"; under "Publishing", the order of the steps as they are built, what a
cancellation and a refused step leave, and that a repair tries its hold file once;
under "Three kinds of lock", how the directory's lock is taken; under "Limits", the
last sync of a publication, the tries for the directory's lock, and a link where an
installation's folder belongs. Changed: "The pointer" and "Publishing" are code
(`install.rs`, `status.rs`) and name their tests; the crashes of "For later tasks" are
the second pull request's. Words, from the review of task M2: what a download asks
for and keeps, what a refused staging removes, a link where a fixed folder or a lock
file belongs, and that a disk's refusal of a piece is not a file that is too long.
Amended: 2026-11-14, after the review of that pull request. Changed, in code and
here: nothing is deleted, and no install is reported done, on the word of a name that
is not durable ("Publishing"); an identity that is on the disk already and whole is
not downloaded again; a publication refused after its folder was renamed in leaves
the folder for the next install of that identity. Words: what a refused repair
leaves, the limit on the last sync, and what the status says then.
Amended: 2026-11-14, with the second pull request of task M2b. Tests only, named
`crashes::…`: under "For later tasks", the crashes of an install name the tests that
hold them, with what a crash does not show; a refused step and then a crash; a
process that dies inside a download; and two installers over one directory, in one
process and in two. Words, from the review of the first: under "Publishing", which
syncs come before the rename of the staged folder and which after, and what finding
an identity whole reads; under "Limits", a held repair whose pointer could not be put
back; and that an install refused for a hold says held, not busy.
Amended: 2026-11-15, with task M3. Added: "Attaching and cancelling", which is code
(`install.rs`, `install_or_attach`; `stage.rs`). Changed, in code and here: one who
attaches tries for a share of the model's lock, paced by its caller, where this
record said that it waits ("Three kinds of lock"); every install writes `installing`
as its first step, one that finds its identity on the disk included ("Publishing",
"The status record"); a cancellation is asked before each file, at each piece and
after each answer of the downloader ("Downloading into staging").
Amended: 2026-11-15, with task M5. Added: "Holds" is code (`hold.rs`), with the
tests named `holds::…`: a hold is asked of the holds of a process (`Holds`), which
count; what each refusal of a hold is. Words and tests, from the review of task M3:
under "Attaching and cancelling", the test of a request that finds no status or one
it cannot read; under "Limits", an install that is refused every write of its
status, and a request that met only another's share of the model's lock.
Amended: 2026-11-15, with task M5b. Added: "Removal" is code up to its decision
(`remove.rs`), with the tests named `remove::…`; the mark is a record, `draining/1`.
Changed, in code and here: the status record is `status/2`, which names the
installation an install is of and has two states of a removal, and a request that
attached is answered busy after a removal and after an install of another identity
("The status record", "Attaching and cancelling"); a removal waits for the holds
before it clears the pointer, where this record said that it clears the pointer
first and puts it back, and it waits for the holds on every installation of the
model that is on the disk ("Removal", with the reason; "Three kinds of lock").
Amended: 2026-11-15, with task M5d. Added, in code and under "Removal": an install
reads the mark where it asks its cancellation and stops as draining; whoever takes
the model's lock to install takes off a mark it finds, and a removal sets its own
again; what "does not go on beside it" means. Tests named `remove::…`.
Amended: 2026-11-15, with task M5c. Added, in code and under "Removal": a removal
deletes every installation of the model under its hold files once the cleared
pointer is durable, and one that finds no pointer deletes what was left; how the
crashes of a removal are held, and what no crash shows.
Amended: 2026-11-15, after the review of task M5d's pull request. Changed, in code
and here: a removal takes off only a mark that is its own, one it made where none
was or one it has under the model's lock, where a busy removal took off the mark
of the removal that had the lock; and a removal sets its mark again while it waits
for a hold. Words: three limits, under "Removal".
Amended: 2026-11-15, with task M6. Added: "The inventory", which is code
(`inventory.rs`), with the tests named `inventory::…`.
Amended: 2026-11-15, after the review of task M5c's pull request. Added, in code
(`directory.rs`, `unlinked`; `Fs::is_link`) and here: "A folder of the layout that is
a link", with the tests named `links::…`. Changed, in code and here: a removal, an
install, a staging and a hold refuse a model one of whose layout folders is a link,
where each went through it, and a removal and an install's cleaning deleted through
it; "nothing outside the models directory is touched" was false of that case, and
is corrected under "Removal": on the disk; the limit on a link where a fixed folder
belongs says what is still followed. Words: that the hold files of deleted
installations are kept, and why.
Amended: 2026-11-15, after the review of task M6's pull request. Changed, in code
and here: the inventory answers damaged for a model one of whose layout folders is
a link ("The inventory"; "A folder of the layout that is a link", what is not
refused). Tests and words: a pointer or a record that cannot be read is damaged,
by a refused call and by a refused permission; two limits of the inventory are on
`docs/carry-over.md` with a milestone. And, from the second review of task M5c's
pull request: a removal looks for a link again after its wait for the holds and
just before it deletes; the look in the model's own unstaging, which the
directory's unstaging makes too, is gone.
Amended: 2026-11-15, with task M6b, in two pull requests. Added: "The lease", which
is code (`inventory.rs`, `Installations::hold` and `Lease`), with the tests named
`leases::…`. The first built the hold a lease takes, under the heading "What a lease
takes"; the second, after the second half of task I7, the lease that stands in the
lease's place of the executor (ADR 0015), and its tests go through an executor.
Amended: 2026-11-15, with task W1 (ADR 0015, "The binary wired"). Changed under "The
inventory" and "The lease": what a config offers is put over an inventory, and the
binary's wiring takes the inventory and puts the lease in its place.
Spec: section 3, "Storage"; section 4, "Segmenters"; section 6, "Controls from table-manners"
Amended: 2026-11-15, at the exit check of checkpoint 1B (task Z1): notes after sentences
that said something was still to come; no decision changed.
Amended: 2026-11-15, with task N2 of checkpoint 1C (`docs/plans/m1c-local-pack.md`,
decision D13). Added, in code (`directory.rs`, `ModelsDir::open`; `Fs::who_may_write`
in the store's file-system interface) and here: "A models directory is its owner's
alone", with the tests named `directory::…`. Changed, in code and here: the
directory's lock is asked for until a time of the clock its directory was opened
with, five seconds, where it was tried a count of times and nothing more; the count
stays as a ceiling ("Three kinds of lock"; "Limits"). Words: under "A folder of the
layout that is a link" and "Limits", what is accepted because nobody but the
directory's owner can write it.
Amended: 2026-11-15, with task P5a of checkpoint 1C (`docs/plans/m1c-local-pack.md`,
decisions D5 and D6). Added, in code (`remove.rs`, `hold.rs`, `install.rs`, `stage.rs`)
and under "Removal": "A mark is live only while its remover is, and the count of
removals": the remover's lock, `draining` as the one question everybody asks of a
mark, the count of removals, and the count a hold returns. Changed, in code and here:
a mark whose remover is gone stops no hold, no publication and no install, where it
stopped all three until someone installed or removed the model; an install takes off
only the mark of a remover that died, where it took off any; a mark is a removal's
own when the removal has the remover's lock, where it was its own when the removal
made it or had the model's lock; of two removals of a model the second waits for
the remover's lock and sets no mark. "The directory", "Three kinds of lock", "Holds",
"Removal" and "Limits" say so where they said otherwise.
Amended: 2026-11-15, with task N3a of checkpoint 1C. Changed, in code and here: the
catalogue is `catalogue/2`, whose entry has a title and the platforms it runs on, and
`catalogue/1` is refused as any other format is ("The catalogue"); the inventory
answers that this build has no adapter, installed or not, where it said out of date
or not installed ("The inventory"). Added: "Where the directory is", with the
binary's wiring of the inventory. From the review of task N2: who whizbang runs as is
read from a pair of sockets, where it was read from a file in the system's temporary
folder, and a failure to learn it has a refusal of its own and is never read as a
directory that is not there; the test that no product code names `ModelsDir::at`
reads every product crate and is not passed by another name for the type; four
limits; and two sentences that said nothing in the library sleeps, which the lock
asked for until a time had made untrue.
Amended: 2026-11-16, with task N3b of checkpoint 1C. Added, in code and under "Where
the directory is": the commands `whizbang models list`, `get` and `remove`
(`crates/whizbang/src/model_commands.rs`), which of them works out where the
directory is and when, and that a relative `--models-dir` is refused before anything
is read or made, where it made a folder in the working folder and was then refused.
Changed, in code and under "The inventory": the inventory answers that the model does
not run on this platform when its entry does not name the platform the build is for
(ADR 0015's sixth reason), where it called such a model ready; `ModelsDir::has_link`
is the look whoever lists models makes, and `unrunnable` what is said of a model with
no directory to look in. Under "Attaching and cancelling": the command's pause.
Amended: 2026-11-16, with task N4 of checkpoint 1C (D12). Added, in code and under "The
catalogue": an entry may say that a download from one of its origins needs a token,
`needs_token`, an optional field of `catalogue/2`. Under "Downloading into staging":
the installer is handed a credential with its downloader (`install_carrying`,
`install_or_attach_carrying`), and attaches it to a request for its origin and to no
other. Under "The inventory", the reason a model does not run on this platform:
`Installations::on` is what the binary's wiring calls with the platform its models
are to run on, where this record said it was for a test.

## Context

`SPEC.md` section 3 says installed models are owned by the models directory, "which is
outside any store: weights are shared by every store on a machine". Section 4 says a model
is "one adapter and one catalogue entry (files, hashes, licence, size, features)". Section
6 keeps model installs that are "atomic, checked by hash, cancellable", with the "previous
install left in place on failure" and a second request attached to the running install,
and replaces removal of a model in use: "the model drains, then is removed".

The plan of checkpoint 1B (`docs/plans/m1b-policy-and-controls.md`, "Decisions") adds:
models are installed by the binary's own code, with no Python present; the directory has
its own lock and its own atomic publication; holds are file locks, not process ids; an
install is attached to, and a removal drains, from any process; and no network client is
in the build, so every download in 1B is a fake behind the interface of ADR 0012.

The models strand is nine tasks. They need one layout, one record and one identity to
meet at before any of them installs anything. This record decides those. Its first task,
M1, builds the types and nothing that touches a disk.

What was read, at the commits `docs/sources.toml` pins:

- Rascal's `tools/python/src/rascal_tools/installs.py`: a models directory of
  `<model>/<identity>/` folders, each with an `install.json` (the model, the catalogue's
  revision, each file's name, size and SHA-256); the identity, which was the SHA-256 of
  that record as canonical JSON; a `current` file replaced by one rename; a `.held/`
  folder of files that each held a process id; `_check_name`, which took a file's name
  only as one plain name.
- Rascal's `catalogue.py`: an entry's id, its adapter, its licence, its files by
  reference to a second file that held sizes, hashes and addresses, and the checks that
  ids are unique.
- Rascal's `models/cache.py` and the "Installations" part of `tools/python/MODEL.md`.
- Table-manners' `CONTRACT.md`, sections 3.5 and 5.3, and
  `schemas/core/installable.json`: an installable's id (a lower-case letter or digit, then
  lower-case letters, digits, `_`, `.` and `-`, 64 at most), its licence (1 to 200
  characters), and an installation named by a digest written `sha256:` and hex.

Three things in rascal are known to be wrong and are on `docs/carry-over.md`: an
installation recorded the catalogue's revision and not the adapter's, and the adapter was
no part of the identity, so a changed adapter looked like the same installation (R2); a
hold was a process id in a file, judged live by that id alone (R5); and removal signalled
a process id read from a file (R3).

## Decision

The code of this record that exists is `crates/whizbang-backends/src/models/`: `name.rs`,
`catalogue.rs`, `installation.rs`, `json.rs`, `refused.rs`, `directory.rs`, `stage.rs`,
`install.rs`, `status.rs`, `hold.rs`, `remove.rs` and `inventory.rs`. Its tests are in
`crates/whizbang-backends/tests/it/models/`, named below as `names::…`, `catalogue::…`,
`installation::…`, `identity::…`, `refusals::…`, `downloads::…`, `staging::…`,
`publishing::…`, `status::…`, `crashes::…`, `attaching::…`, `holds::…`, `remove::…`,
`links::…`, `inventory::…`, `leases::…` and `directory::…`.

**What is code now, and what is decided for a later task.** Each part below says which
it is. A part marked *decided, not built* has no code and no test yet, and names no
test; the task named builds it, tests it and adds its tests' names here, and this
record is what it is held to. Task M1 came to more lines than one pull request may hold
(the plan, "Order"), so it was three: this record and the names, the catalogue, and the
installation record with its identity.

| Part | State |
|---|---|
| Names; the catalogue; how a record is read; an installation's record; its identity; what a refusal says | Code, with the tests named |
| Downloading into staging | Code, with the tests named (task M2) |
| Publishing; the pointer; the status record; the deleting of what the pointer does not name | Code, with the tests named (task M2b), the crashes of "For later tasks" among them |
| Of the layout and the locks: what an install makes and takes | Code (tasks M2, M2b); each part says which |
| Attaching to an install; cancelling one | Code, with the tests named (task M3) |
| Holds | Code, with the tests named (task M5) |
| Removal to its decision, inside one process; the mark | Code, with the tests named (task M5b) |
| Removal against an install and across processes | Code, with the tests named (task M5d) |
| The deleting of a removed model's files | Code, with the tests named (task M5c) |
| A folder of the layout that is a link | Code, with the tests named (task M5c, after its review) |
| The inventory: readiness from what is installed | Code, with the tests named (task M6) |
| The lease: a hold on the admitted installation, by the declared name, around each request to a local model | Code, with the tests named (task M6b) |
| A models directory is its owner's alone; its lock asked for until a time | Code, with the tests named (task N2 of checkpoint 1C) |
| A mark is live only while its remover is; the count of removals | Code, with the tests named (task P5a of checkpoint 1C), under "Removal" |

### The directory

*Decided; of it, task M2 built `models/<model>/`, the model's `LOCK` taken alone by a
try, and `staging/incoming/files/`; and task M2b built the directory's `LOCK`,
`CURRENT`, `STATUS`, `holds/` with the hold file a publication makes, and
`installations/`; task M5 built a hold's share of a hold file; and task M5b built
`DRAINING`, which a removal writes and takes off. The rest is not built (nothing is left: every part of the layout below is built by the task it names).* A models directory is one folder, given by whoever opens it: no
default place and no variable is read for it here ("Where the directory is" says
what the binary gives). In it:

```text
<models>/
  LOCK                             the directory's lock
  models/
    <model>/                       one model, by its name in the catalogue
      LOCK                         the model's lock
      CURRENT                      the pointer: the identity of the installation in use
      CURRENT.new                  a pointer being written, until it is renamed over CURRENT
      STATUS                       what the installer last said (task M2b)
      STATUS.new                   the same of a status record
      DRAINING                     removal's mark (task M5b)
      REMOVER                      the remover's lock (task P5a of 1C)
      REMOVALS                     the count of removals (task P5a of 1C)
      REMOVALS.new                 a count being written, until it is renamed over REMOVALS
      staging/                     the install in progress (task M2)
        incoming/
          RECORD                   the record, written just before the folder is renamed
          files/
            <file>                 a file being downloaded, or downloaded
      holds/
        <64 hex digits>            the file a hold on that installation locks
      installations/
        <64 hex digits>/           one installation, by its identity
          RECORD                   its record, installation/1
          files/
            <file>                 its files, by their names in the catalogue
```

- **Every name that comes from outside the build stands in a folder that holds nothing
  else:** a model's name under `models/`, a file's under `files/`. So no name from a
  catalogue can be, or shadow, a name whizbang gives (`LOCK`, `CURRENT`, `RECORD`), on a
  disk that tells the cases of letters apart or on one that does not.
- **An installation's folder is named by its identity,** as 64 hex digits
  (`InstallationId::hex`), and holds
  exactly its record and the files the record lists, each a regular file of the size
  and hash the record gives. Nothing else is in it: no lock file, so the whole folder
  can be renamed into place and removed.
- **`holds/` holds hold files and nothing else,** each named by an identity as 64 hex
  digits and empty. The hold file of an identity is made by the install that first
  publishes that identity, under the directory's lock, before the pointer names it. It
  is never removed and never replaced, whatever becomes of the installation: an
  installation of the same identity made later, by a reinstall or a repair, uses the
  same file. A reader takes nothing in
  the directory on trust: a name that is not one this layout gives, a link, a folder
  where a file belongs, and a file longer than its bound are refused, never followed.
- **The content of `DRAINING`, and of `REMOVALS`,** is under "Removal". The
  content of `STATUS` is under "The status record", and the one staging folder under
  "Downloading into staging".

### A models directory is its owner's alone

Code (task N2 of checkpoint 1C: `directory.rs`, `ModelsDir::open` and `OpenRefused`;
`Fs::who_may_write` in the store's file-system interface; tests named
`directory::…`). The plan's decision D13: whizbang makes a models directory readable
and writable by its owner only, and refuses one that anyone else can write.

- **Opened where nothing has the name, the folder is made** with the mode `0700`, as
  every folder under it is, and every file in it with `0600`
  (`whizbang_core::store::fs`, `RealFs`); its name is made durable in the folder
  that holds it, which must be there
  (`directory::a_new_models_directory_is_readable_and_writable_by_its_owner_only`,
  which reads the mode of every name an install made).
- **A folder that is there is asked whose it is before anything else,** and two are
  refused (`OpenRefused`): one that is another user's, whatever its mode, since its
  owner can write it and is not the user whizbang runs as; and one of this user's
  whose mode lets its group, or anyone, write it. The mark that keeps one user from
  deleting another's names in a folder changes nothing: it keeps nobody from adding
  a name. A folder of this user's that others may read, and none of them write, is
  taken as it is: the decision is about who can write.
- **Nothing in a refused folder is read, listed, locked or made.** The one call made
  of the file system is the question. The folder's mode is not changed for its
  user: the refusal says how to mend it, in fixed words, and quotes nothing of the
  folder's name, as no refusal here does: "the models directory can be written by
  its group or by anyone, and is not used; take that away (chmod go-w on the
  folder) and ask again"; "the models directory belongs to another user, and is not
  used; name a folder of your own, or make this one yours (chown) and ask again"
  (`directory::a_models_directory_that_its_group_or_anyone_can_write_is_refused_and_nothing_in_it_is_read`).
- **The question is `Fs::who_may_write`:** the folder's owner and the two bits of its
  mode, read through the name, so that a link at the directory's own folder is
  followed as it is everywhere ("A folder of the layout that is a link"). On the
  test kit's disk it is no event. A file system that does not give the call answers
  that it is unsupported, and a models directory over it is not opened
  (`whizbang-core`'s
  `fs::who_may_write_a_folder_is_read_from_its_owner_and_its_mode_and_a_link_to_it_is_followed`).
- **Who whizbang runs as** is not something the standard library says, and the
  workspace has no crate and no `unsafe` that would ask the system. It is read as
  the owner of something the process has just made, which the system gives to
  whoever made it: one end of a pair of connected sockets (`UnixStream::pair`),
  asked through the open socket itself, which has no name anywhere, touches no
  folder and is closed at once; once in a process, and only when a models
  directory is opened. Until task N3a it was an empty file made in the system's
  temporary folder and removed, which needed that folder to be there and
  writable. That the owner of the socket is the owner of a file and of a folder
  the process makes is held on both platforms, by a test that also asks about a
  real folder of another user, the root of the file system, and is told it is not
  this user's; where the tests are themselves run as the user who owns the root
  there is no such folder to ask about, and that half is not asked
  (`whizbang-core`'s
  `fs::the_real_file_system_says_that_a_folder_of_another_user_is_not_this_users`).
  No test runs anything as another user.
- **A failure to learn it has a refusal of its own** (`OpenRefused::UserUnknown`):
  "which user whizbang runs as could not be learned, so whose the models directory
  is cannot be told, and it is not used; nothing in it was read and nothing was
  made; ask again". The file system asks who its user is before it looks at the
  folder, and says a failure as an error of its own kind (`fs::UserUnknown`),
  whatever the system said: so it is never taken for "nothing has the name",
  which the opening reads as a directory that is still to be made. Until task
  N3a a temporary folder that was not there made an empty directory, which was
  then refused as a failure of the disk
  (`directory::a_failure_to_learn_who_the_user_is_has_its_own_refusal_and_is_never_read_as_a_directory_that_is_absent`,
  `fs::a_failure_to_learn_the_user_is_an_error_of_its_own_kind_and_never_a_name_that_is_not_there`).
- **Two ways to a `ModelsDir`.** `ModelsDir::open` is the one above, and the only one
  the product uses: the binary's wiring opens the directory a command names with it
  ("Where the directory is"), and no source file of a product crate names the
  other. `ModelsDir::at` asks nothing of the folder and is given no clock: it is
  for a folder its caller made, which is a test's. The test reads every source
  file of the six product crates (the test kit is no product), comment lines
  apart, with the white space taken out, and fails on the path `ModelsDir::at`,
  called or not; on `ModelsDir as`, which gives the type another name where it is
  imported; on a `type` whose right side ends in `ModelsDir`; on `Self::at` in a
  file that holds an `impl ModelsDir`; and on a second definition of `at`. It runs
  each of those against its own detector first
  (`directory::the_binary_names_a_models_directory_only_through_the_opening_that_asks_whose_it_is`).
  A scan was chosen over a cargo feature that would compile `at` only for tests:
  `just check` builds every target with the test kit's dependencies, so under it
  the feature would be on in the binary's own build too, and only a build of the
  binary alone would refuse a use; and it would put a feature into the crate graph
  for one function. The scan is a tripwire and not a proof: a macro that builds
  the name from pieces is not seen.

**Limits.** It is a look, when the directory is opened: a mode or an owner changed
afterwards is not seen until the next opening. The folders above the directory are
not looked at: one who can rename a folder above it can put another in its place.
Only the directory's own folder is asked about; what is under it whizbang made, for
its owner only, and a mode its owner changed there afterwards is not looked for.
Lists of who else may reach a folder that a system keeps beside its mode are not
read. And the user who administers the machine can write every folder on it.
**A file system that reports no owner:** some report every folder as the user's
who asks, or as one fixed user, whatever is on the disk (a disk mounted without
owners, some network and shared folders). On the first, a folder another user can
write is opened as this user's; on the second, this user's own is refused as
another's, and the refusal's advice (`chown`) does not help: name a folder on
another disk. Nothing here can tell such a file system from one that keeps
owners. **When the directory's own folder is a link,** the folder asked about is
the one the link leads to, and the link's own owner is not asked: one who can
replace the link, which is one who can write the folder that holds it, chooses
the directory, as one who can rename a folder above it does.

### Where the directory is

Code (task N3a of checkpoint 1C: `crates/whizbang/src/wiring.rs`, `Models`,
`Over::installed`, `models_dir`, `platform_models_dir`; the flag in
`crates/whizbang/src/rascal.rs`; tests named `wired::…`, in
`crates/whizbang/tests/it/wired.rs`). The library is given its directory by whoever
opens it. This is what the binary gives.

- **Three may say where it is, in this order:** the command's flag, `--models-dir
  <DIR>`; then the config's `[models] dir`, an absolute path (ADR 0011, "What the
  file holds"); then the platform's data folder for whizbang. The first that says
  is the directory, whatever the others say
  (`wired::the_models_directory_is_the_flags_then_the_configs_then_the_platforms`).
  With none of the three the command is refused, and says how to name one.
- **The platform's** is `Library/Application Support/whizbang/models` under the home
  folder on macOS; elsewhere `whizbang/models` under the folder `XDG_DATA_HOME`
  names, or under `.local/share` of the home folder when it names none. A folder
  the environment names is taken only when it is an absolute path; a machine with
  no home folder has no such directory, and nothing is guessed. Rascal read a
  variable of its own, then looked beside its pack, then in the user's cache
  folder: none of the three is kept. A cache is what a system may empty, and a
  model is gigabytes a person chose to download.
- **It is opened by `ModelsDir::open` and by nothing else** ("A models directory is
  its owner's alone"), with the machine's clock, so one that another can write is
  refused before anything in it is read, and the refusal says which of the three
  named it and never its path. That opening makes the directory's own folder and
  no folder above it. **The wiring makes the folders above the platform's,** each
  for its owner only: nobody made a data folder for whizbang before its first use.
  It does not make the folders above one a person named, by the flag or in the
  config: a path written wrong is said, not made.
- **The inventory is told the secrets the executor is told,** by the one who gives
  the executor its own: `Over::knowing` gives them to the executor, to the source
  of its config and to the inventory, which is made again over the same directory
  when they are given after it. Nobody else makes the inventory of an executor,
  so the two never know two sets
  (`wired::a_secret_the_executor_is_told_of_is_refused_as_a_models_name_by_the_binarys_inventory`).
- **What the binary gives it** is the catalogue it ships (`wiring::catalogue`),
  which holds no model until its first entry is written (plan 1C, task N5), under
  the adapters it has (`wiring::adapters`), which are none until the first
  (task R2a). A test gives its own catalogue and adapters to the same wiring, and
  a call for an installed model is then prepared under its installation's
  identity and holds a lease while its request is out
  (`wired::under_the_binarys_wiring_a_call_for_an_installed_model_is_prepared_under_its_identity_and_holds_a_lease`);
  one for a model whose adapter the build lacks is prepared as not ready, with
  that reason
  (`wired::a_call_for_a_model_whose_adapter_this_build_lacks_is_refused_when_prepared_with_that_reason`).
- **Where no model could be asked for, no directory is opened or made:** under a
  catalogue that holds no model, since the inventory of it reads no directory; and
  for a command that names neither a config nor a directory, whose executor has no
  backend to ask for one. Such an executor is over the inventory that knows no
  model, as every executor was before this task.
  **Amended by task N5 of plan 1C:** and under a build that has no kind of backend
  (`wiring::kinds`, none until the kind `pack`, task P6a), since no config of it can
  offer a backend, and so none can ask for a model. With the shipped catalogue's
  first entry, `whizbang rascal grow` given a config or `--models-dir` would
  otherwise have made a models directory, in the platform's place when nothing named
  one, to be told of every model what the inventory says already. So until a kind
  is in the build, `rascal grow` opens and makes no models directory and asks
  nothing of where the platform's is
  (`wired::with_the_catalogue_it_ships_the_binary_opens_and_makes_no_models_directory`,
  which fails on the day the build is given a kind), and only the `models` commands
  look at one. The bullet below on which commands open a directory is read with this.
- **The platform's is not even worked out unless it is the one.** The wiring is
  given what says where it is (`Models::platform`), and asks it only when a
  directory is about to be opened and neither the flag nor the config named one:
  never under a catalogue of no model, never for a command that names neither a
  config nor a directory, and never when one of the other two said. The test
  counts the askings.
- **Which commands open a models directory, and which ask for the platform's.** At
  this head, under the catalogue the build ships: none opens one and none asks.
  Two commands are handed what says the platform's place: `whizbang rascal grow`,
  the only one that builds an executor, and `whizbang models` (task N3b);
  `describe`, `store`, `runs`, `spend` and the two stubs are handed nothing and
  have no way to it. Once the catalogue holds a model, `rascal grow` opens a
  directory when it is given `--config` or `--models-dir`, and asks for the
  platform's only when it is given a config that has no `[models] dir` and no
  `--models-dir`. The `models` commands work out where it is only when the
  catalogue gives them a reason, and each asks for the platform's only when
  neither the flag nor the config named one
  (`models::under_the_catalogue_it_ships_no_models_command_makes_a_models_directory_or_asks_anything_of_any_address`):
  `list` when the catalogue holds a model, and it opens the directory only when
  something is at its place, so it makes nothing; `get` of a model the catalogue
  holds, that runs on this platform and whose adapter the build has, and it makes
  the directory where it is not; `remove` of a model the catalogue holds, and it
  opens the directory only when something is at its place. **A relative
  `--models-dir` is refused by every command that takes it,** before anything is
  read or made, whether or not the command would have looked at the directory
  (`models::a_relative_models_dir_is_refused_before_anything_is_made`); until task
  N3b it was opened relative to the working folder, which made a folder there and
  then refused it.
- **No test touches the data folder of whoever runs it.** The function that works
  out the platform's place reads nothing: it is handed what the environment says,
  and its test hands it made-up values. One function of the binary reads the
  environment; `main` alone names it, and hands it on without calling it; and
  nothing else in the binary reads the environment at all
  (`wired::only_main_reads_where_the_platforms_data_folder_is`). A test of the
  wiring gives its own, which answers a place in the test's scratch folder, or
  gives none. That leaves a test that starts the binary itself, which inherits
  the tester's environment unless it clears it: none can reach the folder today,
  because the shipped catalogue holds no model, so no command opens a directory
  or asks where the platform's is. The test that holds that starts the binary
  with a cleared environment, a temporary home folder and the directory named,
  by the flag and then by the config, and fails on the day the catalogue is
  given an entry
  (`wired::with_the_catalogue_it_ships_the_binary_opens_and_makes_no_models_directory`).

**Limits.** The config is read once for `[models] dir`, when the executor is built
over it: the source reads the config again at every ask, and a directory named in
it afterwards is not seen by an executor already built, which a command that runs
once does not meet and a server that stays up will (milestone 5,
`docs/carry-over.md`). The secrets the inventory is told are those given from the
start: the value of a variable the env file holds is the executor's secret from
the first call on (ADR 0011, "The source over both files") and is not the
inventory's; a model's name is a `ModelName` from a catalogue the build ships, and
the config that would ask for a model by a key's value is refused whole by its
source. A config whose backends are all remote still opens the directory, and is
refused when it cannot be: where the directory is asked about only when a local
model is first looked for, the refusal that says how to mend it would have no
place to be said. The folders above the platform's are made with the mode `0700`
where they are made; ones that are there are left as they are, and are not asked
whose they are ("A models directory is its owner's alone", limits). And `~` in a
path is nobody's home: the flag's is what the shell made of it, and the config's
is refused.

### Three kinds of lock, and their order

*Decided; of it, task M2 built the model's lock taken alone by a try, task M2b
the directory's lock around a publication, the making of a hold file, and a hold file
taken alone by whoever deletes an installation's folder, and task M3 the model's
lock shared by one who attaches, and task M5 a hold's share of its hold file. The
rest is not built: the mark (built since, by task M5b: "Removal").* All three are the locks of the store's file-system interface
(`whizbang_core::store::fs`), which are `flock` on Linux and macOS, and everything
`store/fs.rs` says of them holds here: a models directory on a network file system is
not supported, and **no lock file is ever removed or replaced:** not the directory's
`LOCK`, not a model's `LOCK`, not a file under `holds/`. There is no exception. The row
of `docs/carry-over.md` on those locks names the models directory too.

- **The directory's lock** (`LOCK`) is taken alone, for a moment, around every change of
  a pointer or of a draining mark, around the making of a hold file, and around the
  reading of the pointer and the mark that a hold or a publication decides from. It is
  never held across a download or while anything is waited for
  (`publishing::under_the_directorys_lock_a_publication_makes_the_hold_file_and_replaces_the_pointer_and_no_more`).
  **How it is taken: asked for until a time** (task N2 of checkpoint 1C; until
  then it was tried 20,000 times with nothing between two tries but giving way to
  other threads, and under a holder that was slow to sync a publication could be
  refused within a moment). The store's interface has no lock taken alone that
  waits, and a lock that blocks without an end is not used: a lease and a
  removal's wait must each end. So the lock is tried; when it is not given, the
  clock the directory was opened with is asked to let a millisecond pass
  (`Clock::pause`), and it is tried again, until five seconds of that clock have
  passed since the first try. One who is not given it by then is refused as busy,
  and asks nothing more of the clock; a publication has changed no pointer, and a
  repair may have cleared one
  (`directory::a_directory_whose_lock_is_held_past_five_seconds_of_the_clock_answers_busy_and_nothing_waits_longer`,
  `publishing::a_directory_whose_lock_is_not_let_go_refuses_the_publication_and_names_what_it_named`).
  Whoever has the lock lets it go after a few calls of the file system, and one
  who came meanwhile is then given it
  (`directory::two_publications_at_once_both_end_and_neither_is_refused_as_busy`).
  **The machine's clock sleeps through a pause; a test's does not,** and counts it
  or moves itself: this is the one place in the models directory's code where
  anything waits by itself, and between two tries for any other lock what comes is
  still the caller's ("Attaching and cancelling", "Removal"). A lock that is free
  is given at the first try, before anything is asked of the clock.
  **The 20,000 tries stay, as a ceiling.** A clock that stands still, as a test's
  does, never comes to the time, and the tries must end all the same: at the
  20,000th the answer is busy whatever the clock says
  (`directory::under_a_clock_that_stands_still_a_held_lock_answers_busy_at_the_ceiling_of_tries`).
  Under a clock that keeps time the time comes first: five seconds hold 5,000
  tries a millisecond apart. A directory that was given no clock (`ModelsDir::at`,
  a test's) has the ceiling alone, and gives way to other threads between two
  tries, as before.
- **A model's lock** (`models/<model>/LOCK`) is taken alone by one install of that
  model, for the whole of it, a download included; and by a removal for its work on
  the disk, after its mark is set ("Removal", below). So one install of a model runs
  at a time, and whoever asks for a model that is being installed attaches by
  trying for a share of this lock until it is given one ("Attaching and
  cancelling"): a try does not wait, and what comes between two tries is its
  caller's. Staging is written, and cleaned, only by its holder.
- **A hold file** (`models/<model>/holds/<64 hex digits>`) is shared by each hold on
  that installation, and taken alone by whoever deletes the installation's folder.
- **The remover's lock** (`models/<model>/REMOVER`; task P5a of checkpoint 1C, so
  the kinds are four) is taken alone by a removal of that model, by a try, before
  its mark is set, and kept until the mark is taken off. Whoever asks whether a mark
  is live tries for a share of it and lets the share go at once ("Removal": "A mark
  is live only while its remover is").

**The order.** What a process may wait for depends on what it holds:

| Holding | May wait for | Never |
|---|---|---|
| Nothing | The directory's lock | A lock taken alone is tried and never waited for: the remover's lock by a removal, the model's lock by an install or a removal, a hold file by whoever deletes. A model's lock is shared by a try too, tried again by one who attaches |
| The remover's lock, alone | The directory's lock; and what a holder of the model's lock may, once it has that too | Taken first of all, holding nothing. Waits for no other lock: the model's lock and the hold files are tried, as before |
| Anything, or nothing | Nothing more | A share of the remover's lock is a try that does not wait, by one who holds nothing, the directory's lock, or the model's lock and the directory's; it is let go before anything else is done, so nothing is ever waited for under it |
| The directory's lock | Nothing | Takes no model's lock. Takes a hold file only to share it, as below, which does not wait. Waits for no hold |
| A model's lock, alone | The directory's lock | Waits for no hold file: it takes one alone by a try that does not wait, or not at all |
| A model's lock, shared (attached) | Nothing | Takes no other lock: it reads the status record and the pointer, and lets its share go |
| A hold file, alone, by an install that deletes | Nothing | Takes no lock and waits for none: it deletes the folder, syncs, and lets the hold file go before it takes the directory's lock or sets a pointer |
| A hold file, alone, by a removal | The directory's lock, to clear the pointer | Sets no pointer. Its mark is set, so nobody is given a hold on that model under the directory's lock meanwhile |
| A hold | The directory's lock, to take another hold | Never waits for that model's lock, alone or shared: a run that holds a model neither installs it, removes it nor attaches to its install |

- **A hold is taken only under the directory's lock,** after the pointer was read and
  names that installation and no draining mark is set whose remover is alive. Its share of the hold file is a
  try that does not wait, and is not refused there: a hold file is held alone only while its installation's folder is
  being deleted, by a holder of the model's lock, after the pointer stopped naming
  that installation; and the pointer cannot name it again until that holder has let
  the hold file go, which it does as soon as the folder is deleted, before it takes the
  directory's lock again and before any pointer is set. Or it is held alone by a
  removal, which has set the mark: a hold that is asked for then is refused for the
  mark, before the hold file is tried.
- **A hold file is taken alone only by a try that does not wait,** by a holder of the
  model's lock, and only for an installation the pointer does not name, or, by a
  removal, for the installations of a model whose mark is set
  (`remove::a_removal_marks_under_the_directorys_lock_says_removing_and_clears_the_pointer_with_every_hold_file_its_own`).
- So the directory's lock is last in every chain, and its holder waits for nothing;
  nobody waits for a hold; and a holder of a hold never waits for the model's lock.
  The remover's lock is first in the one chain it is in, and is never waited for:
  alone or shared, it is a try. There is no cycle.

### Names

Code. Four kinds of text come from outside the build, from a catalogue or from a record
read back. Each has a type with a private field, made by a constructor that checks it:

| Type | Rule |
|---|---|
| `ModelName`, of a model and of an adapter | A lower-case letter or a digit, then lower-case letters, digits, `_`, `.` and `-`; 64 characters at most (`MODEL_NAME_LIMIT`). Table-manners' rule for an installable's id |
| `FileName` | A letter or a digit, then letters, digits, `_`, `.`, `-` and `+`; 128 characters at most (`FILE_NAME_LIMIT`) |
| `Revision` | A letter or a digit, then letters, digits, `_`, `.` and `-`; 64 characters at most (`REVISION_LIMIT`) |
| `Licence` | 1 to 200 printable ASCII characters and spaces (`LICENCE_LIMIT`), with no space at either end |

- A model's name is the name of a folder and a file's is the name of a file. Each starts
  with a letter or a digit and holds no separator, so it is **one plain name**: never
  empty, never `.` or `..`, never hidden, never a path, and never an option to a command
  (`names::a_models_name_is_lower_case_letters_digits_and_three_marks_and_starts_with_none_of_the_marks`,
  `names::a_files_name_is_one_plain_name`).
- A model's name is in lower case only, so that two models are two folders on a disk that
  does not tell the cases apart. A file's name may hold upper case, as `LICENSE.txt`
  does; two files of one model whose names differ only in the case of their letters are
  refused as one name twice
  (`catalogue::an_entry_holds_one_to_a_bounded_count_of_files_each_named_once_in_either_case`,
  `installation::a_record_holds_one_to_a_bounded_count_of_files_each_named_once_in_either_case`).
- (`names::a_revision_is_letters_digits_and_three_marks`,
  `names::a_licence_is_one_line_of_printable_ascii`.)
- **Each constructor is told the executor's secrets and looks at the whole text for one
  before it judges it by its rule.** A text that holds a secret is refused as
  `Refused::HoldsSecret`, whatever else is wrong with it, and nothing of it is shown
  (`names::a_name_that_holds_a_secret_is_refused_whole_before_its_rule_and_not_shown`).
  No folder and no file is named but from a `ModelName`, a `FileName` or the hex of an
  installation's identity, so a name that holds a secret the reader was told of never becomes a
  name on a disk. Nothing writes to a disk yet: the tasks that do each search every name
  and every byte they wrote for a planted secret. (They write since task M2, and do.)
- **What is shown of a name** is its first 64 characters (`SHOWN_LIMIT`), and three dots
  when there are more. Every character a name may hold is printable ASCII, so nothing of
  it needs an escape. It is what `{:?}` prints
  (`names::what_is_shown_of_a_name_is_its_first_characters_and_three_dots_when_there_are_more`).

### The catalogue

Code. A `Catalogue` is the models that can be installed: at most 256 (`MODELS_LIMIT`), no
two of one name, kept in the order of their names
(`catalogue::a_catalogue_holds_a_bounded_count_of_models_each_named_once`). An `Entry` is
one model:

| Field | What it is |
|---|---|
| `name` | The model's name, a `ModelName` |
| `title` | The model's title, as it is shown to a person, a `Title`: 1 to 100 printable ASCII characters and spaces (`TITLE_LIMIT`), with no space at either end. It names nothing on a disk and is compared with nothing |
| `revision` | The revision of this entry, a `Revision`. Whoever changes anything of an entry changes its revision |
| `adapter` | The adapter that runs the model, by its name, a `ModelName` |
| `licence` | The licence the files are under, as their maker states it |
| `platforms` | The platforms the model runs on: 1 to 8 (`PLATFORMS_LIMIT`), each named once, each a `Platform`. There are two, the platforms of `SPEC.md` section 1 under the names rascal's catalogue gave them: `macos-arm64` and `linux-x86_64` |
| `features` | The features the adapter gives with this model: at most 16 (`FEATURES_LIMIT`), each a feature's name as a backend declares one (ADR 0006), each stated once |
| `origins` | The origins a download of the files may reach: at most 8 (`ORIGINS_LIMIT`), each named once |
| `files` | 1 to 64 files (`FILES_LIMIT`), each with its `name`, its `size` in bytes, its `sha256`, and `from`, the address it is first asked for at |
| `needs_token` | May be left out. One of the entry's `origins`, written as an origin, whose downloads need a token (task N4, D12; `catalogue::an_entry_may_say_that_a_download_from_one_of_its_origins_needs_a_token`). The token itself is never in a catalogue: where it is, is the config's (ADR 0011, "A download's token"), and which requests carry it is ADR 0012's, "A download's token". It is no part of an installation's identity: it changes no byte that is installed |

(`catalogue::a_catalogue_is_read_as_its_models_and_their_files`,
`catalogue::an_entry_holds_a_bounded_count_of_features_each_a_name_declared_once`,
`catalogue::every_other_field_of_an_entry_keeps_its_rule`; the title and the
platforms:
`catalogue::an_entry_of_the_second_version_holds_a_title_and_its_platforms_and_the_first_version_is_refused`.) What an entry is made from,
`EntryParts`, holds features that no constructor has looked at yet, so what `{:?}`
prints of it is its names and three counts
(`catalogue::what_is_printed_of_the_parts_of_an_entry_is_counts_and_never_a_feature`).

- **A file's size** is 1 byte to 1 TiB (`FILE_MAX_BYTES`). It is what a download is
  stopped at, so a size of zero is refused: it would stop a download before its first
  byte and prove nothing
  (`catalogue::a_size_of_zero_is_refused_and_so_is_one_past_the_most_a_file_may_be`).
- **A file's hash** is written as every digest is (ADR 0001): `sha256:` and 64 lower-case
  hex digits (`catalogue::a_hash_of_the_wrong_length_is_refused`).
- **Where a file may be downloaded from is two things.** `from` is an `Address` (ADR
  0012): where the first request for the file goes. `origins` is every origin a download
  of this model's files may reach, the first address's and any it is moved to, since a
  download does not follow a redirect and hands back the address it was pointed to (ADR
  0012, "Three interfaces"). A file whose `from` is at an origin the entry does not name
  is refused, and `Entry::may_download_from` is the question the installer asks of each
  address it is moved to (task M2). An origin is written as an origin, with no path
  after it
  (`catalogue::an_entry_names_a_bounded_count_of_origins_once_each_and_every_file_comes_from_one_of_them`).
- **A title and the platforms are in the entry and not in an installation.** An
  installation's record and its identity are made from the entry's name, its
  revision, its adapter and its files ("An installation and its record"), as
  before: a title corrected alone, against the rule that whoever changes an entry
  changes its revision, is the same installation. `Entry::runs_on` is the
  question the command that gets a model asks (plan 1C, task N3b), and
  `Platform::this` is the platform a build is for, or none where it is for
  neither. The inventory does not ask ("Limits").
- **Platforms, features, origins, files and models are kept in the order of their texts,** whatever
  order they were written in. Every such text is ASCII, so the order of its bytes and
  the order of its characters are one order; "the order of their names" and "the order
  of the bytes of their names" in this record mean that one.

**The file.** A catalogue is a file a person writes, in JSON, under the format
`catalogue/2`. The first format, `catalogue/1`, had no title and no platforms; no
catalogue of it was ever shipped, so it is not read: it is refused for its format,
as any format but the one is, and nothing converts one.

```json
{
  "format": "catalogue/2",
  "models": [
    {
      "name": "tiny",
      "title": "Tiny, for tests",
      "revision": "2026-11.1",
      "adapter": "owlv2-sam2",
      "licence": "Apache-2.0",
      "platforms": ["macos-arm64", "linux-x86_64"],
      "features": ["phrase", "box"],
      "origins": ["https://weights.example.org"],
      "files": [
        {
          "name": "graph.onnx",
          "size": 3,
          "sha256": "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
          "from": "https://weights.example.org/files/graph.onnx"
        }
      ]
    }
  ]
}
```

It is at most 1 MiB (`CATALOGUE_MAX_BYTES`). A person may write it in any way JSON
allows of saying the one thing: with any white space, the fields in any order, a
character spelled as an escape; and an address's host in either case, its port written
or not (`catalogue::a_person_may_write_a_catalogue_in_any_way_json_allows`). What
whizbang writes of one (`Catalogue::to_bytes`) is written one way: compact, the fields
in the order above, and an address as its origin and the rest of it; a catalogue reads
back from that as itself (`catalogue::a_catalogue_round_trips`).

**A catalogue that could not be read back is not made.** `Catalogue::new` refuses one
whose written form is over the bound
(`catalogue::a_catalogue_too_long_to_be_read_back_is_not_made`), and one whose written
form holds a secret. The written form is looked at whole, because it puts side by side
what a person wrote apart, and writes an address without a port that is its scheme's
own, which brings its host and its path together
(`catalogue::a_secret_that_writing_the_catalogue_would_spell_is_refused_where_the_catalogue_is_made`).
A part that holds a secret is refused where the part is made, a file's address and a
feature among them, and a secret that only the lowering of a host makes (ADR 0012) is
refused as a secret
(`catalogue::a_part_that_holds_a_secret_is_refused_where_the_part_is_made`).

### The shipped catalogue

Added by task N5 of plan 1C. The catalogue the binary ships is the file
`crates/whizbang/src/catalogue.json`, compiled in and read by `wiring::catalogue` as any
catalogue is. It holds one model, `sam3`:

- **Its files** are those of D11 of plan 1C at one pinned commit of `facebook/sam3` on
  the Hub (`3c879f39826c281e95690f02c7821c4de09afae7`, the head of `main` when it was
  pinned): `model.safetensors` and the small text files the runtime reads beside it
  (`config.json`, `processor_config.json`, the four tokenizer files) and `LICENSE`.
  `sam3.pt`, a pickle, is not listed, nor the card and `.gitattributes`. Each file is
  first asked for at `https://huggingface.co/facebook/sam3/resolve/<commit>/<name>`.
- **Its origins** are `https://huggingface.co` and `https://us.aws.cdn.hf.co`, the
  host the Hub moved the large file's download to when it was pinned (open question 2
  of plan 1C), named to the owner and approved with the Hub's storage. The token goes
  to the first alone (ADR 0012).
- **`needs_token`** is `https://huggingface.co`; the licence is `SAM License`, as its
  maker names it; the platform is `macos-arm64` alone, the one platform 1C installs
  and runs on; the feature is `phrase`, which the adapter of 1C gives.
- **How the sizes and hashes were fixed.** Every file's size is the one the Hub's
  public listing of the pinned commit states
  (`https://huggingface.co/api/models/facebook/sam3/tree/<commit>`, read once without
  a token). Every file's SHA-256, the large file's included, is what
  `whizbang models pin` worked out from the bytes it fetched once, through the checked
  downloader with the owner's token: trust on first use. The plan had the large file's
  hash from that listing, which states the SHA-256 of a large file as `lfs.oid`; for
  this gated repository the Hub masks it, and its Xet hash, to a reader without a
  token, so the listing could not give it. The first `get` on the owner's Mac then
  downloaded the files again and held each to its size and hash, which they kept.
- **The table of adapters** (`wiring::adapters`) states, for each adapter, the
  revision it is installed under and whether this build's pack ships it. `sam3` is at
  revision `1`, not yet shipped: `get` installs the model under that revision, and the
  inventory is told only of shipped adapters, so it calls `sam3` not ready, for want
  of an adapter (ADR 0015's fifth reason), installed or not, until the pack ships the
  adapter (task R2a) and the table says so. The revision is part of the
  installation's identity; changing whether the adapter is shipped changes nothing of
  it, so nothing is downloaded again then
  (`inventory::a_model_whose_adapter_the_binarys_table_names_as_not_yet_shipped_is_not_ready_for_want_of_an_adapter_installed_or_not`).
  A table that names an adapter as shipped which the pack does not have, or as not
  shipped which it has, fails
  `catalogue::every_adapter_the_binarys_table_names_as_shipped_is_an_adapter_the_pack_has`.
- **Any change to the entry is a new revision** (below, "An installation and its
  record"), and so a second download of 3.44 GB. Linux is added with the runtime on
  Linux (1D) under a new revision.

### How a record is read

Code (`json.rs`). Stored bytes are not trusted, a catalogue's and an installation
record's alike. In this order:

1. **The length.** Bytes over the bound are refused as `Refused::TooLarge` before
   anything of them is looked at. Read from a source (`read_from`), no more than the
   bound and one byte is asked of it, however much it has; a source that fails is
   refused by the kind of its failure, with no path
   (`catalogue::a_catalogue_over_its_bound_is_refused_before_it_is_read`,
   `catalogue::reading_a_catalogue_asks_its_source_for_no_more_than_the_bound_and_one_byte`,
   `catalogue::a_catalogue_that_cannot_be_read_is_refused_by_the_kind_of_failure_and_no_path`,
   `installation::an_installation_record_over_its_bound_is_refused_before_it_is_read`,
   `installation::reading_a_record_asks_its_source_for_no_more_than_the_bound_and_one_byte_and_keeps_no_path`).
2. **A secret, in the bytes as they are.** Bytes that hold a secret the reader was told
   of, anywhere, are refused whole.
3. **JSON.** Bytes that are not JSON, that have anything after the one value, or that
   nest deeper than the format's reader follows (128 levels) are refused by their line
   (`catalogue::a_catalogue_that_is_not_json_is_refused_by_its_line`,
   `installation::a_record_that_is_not_json_is_refused`,
   `refusals::values_nested_past_what_the_reader_follows_are_refused_and_not_followed`).
4. **A field given twice** in one object, at any depth, is refused by its line. The usual
   reader keeps the last and says nothing
   (`catalogue::a_catalogue_with_a_field_given_twice_is_refused_by_its_line`).
5. **A secret, in every key and every text as JSON decoded it:** an escape can spell
   what the bytes do not hold
   (`catalogue::a_catalogue_that_holds_a_secret_anywhere_is_refused_whole_and_not_shown`,
   `installation::a_record_that_holds_a_secret_anywhere_is_refused_whole_and_not_shown`).
6. **The fields, by hand.** Every object is an object with its fields by name; a field
   that is not one of its fields, a field that is missing, and a value of another kind
   than its field's are each refused, with the place
   (`catalogue::a_catalogue_with_an_unknown_field_or_a_missing_one_is_refused`,
   `catalogue::a_catalogue_with_a_value_of_another_kind_is_refused`,
   `installation::a_record_with_an_unknown_field_a_missing_one_or_one_given_twice_is_refused`).
   A size is a whole number from 0 to what 64 bits hold, written as one: a fraction, a
   negative number and a text are another kind. Every list's count is looked at before
   any of its items is
   (`catalogue::a_list_over_its_bound_is_refused_for_its_count_before_any_of_its_items_is_looked_at`,
   `installation::a_list_of_files_over_its_bound_is_refused_for_its_count_before_any_file_is_looked_at`).
   The format is the one the reader reads, or the record is refused
   (`catalogue::a_catalogue_of_another_format_is_refused`,
   `installation::a_record_of_another_format_is_refused`).
7. **Every value by its rule,** through the same constructors that make a value in
   memory, so nothing is read that could not have been made
   (`catalogue::a_file_name_that_is_not_one_plain_name_is_refused`,
   `installation::a_record_with_a_file_name_that_is_not_one_plain_name_is_refused`,
   `installation::a_record_with_a_hash_of_the_wrong_length_is_refused`,
   `installation::a_record_with_a_size_of_zero_is_refused_and_so_is_one_past_the_most_a_file_may_be`).

### An installation and its record

Code. An **installation** is one model's files as one entry of a catalogue listed them,
installed under one revision of its adapter. Its record, `InstallationRecord`, holds:

| Field | What it is |
|---|---|
| `model` | The model's name |
| `catalogue_revision` | The revision of the catalogue's entry it was installed from |
| `adapter` | The adapter's name |
| `adapter_revision` | The revision of the adapter it was installed under |
| `files` | 1 to 64 files, each with its `name`, its `size` and its `sha256`, in the order of the bytes of their names |

- **It holds both revisions,** which is what rascal's did not (R2): task M6 reports an
  installation as out of date when either is not the current one. The catalogue's
  revision is the entry's. The adapter's is stated by the adapter, and is given to
  `InstallationRecord::of` beside the entry by whoever knows the adapter
  (`installation::an_installation_record_holds_the_model_its_files_and_the_two_revisions_it_was_made_under`).
- **It holds nothing of where a file came from,** nothing of when or by whom it was
  installed, and no path. The licence and the features stay in the catalogue, under the
  entry's revision.
- **Any change to an entry is a new revision, and so a new identity.** A corrected
  licence line, a feature added, an address moved: whoever changes the entry changes
  its revision, the revision is in the record, and the record is the identity. An
  installation of the old revision is then reported as out of date (task M6) and
  installing the model again makes a new installation of what may be the same files.
  That is intended: an installation says which entry it was made from, whole.
- **Whizbang writes it, in one form, and reads back no other.** The form is compact
  JSON under the format `installation/1`, the fields in this order, the files in the
  order of their names, nothing after the last brace:

  ```json
  {"format":"installation/1","model":"tiny","catalogue_revision":"2026-11.1","adapter":"owlv2-sam2","adapter_revision":"7","files":[{"name":"LICENSE.txt","size":5,"sha256":"sha256:51d2361f4faea3bc8f9facdbc7d99abb555596a2e51f7b25fd3b41c93587e616"},{"name":"graph.onnx","size":3,"sha256":"sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}]}
  ```

  Bytes that say what a record says in any other way (white space, the fields or the
  files in another order, a character spelled as an escape) are refused as
  `Refused::OtherForm`: the record is read, written again, and the two compared
  (`installation::a_record_is_read_back_only_in_the_one_form_whizbang_writes`,
  `installation::an_installation_record_round_trips`).
- **It is at most 32 KiB** (`INSTALLATION_MAX_BYTES`). The longest record there can be,
  every name at its limit and the most files of the largest size, is 15,979 bytes, so no
  record that can be made is refused when read back
  (`installation::the_longest_record_whizbang_writes_is_under_the_bound_it_reads_back`).
- **A record that would spell a secret as it is written is not made**
  (`installation::a_record_that_would_spell_a_secret_is_not_made`).

### The identity of an installation

Code. An installation is named by `InstallationId`: the SHA-256 of one encoding of its
record, hashed as ADR 0001 hashes every identity ("How an identity is hashed"). The
kind is `installation/1`. The bytes hashed are:

- the bytes `whizbang:installation/1` and a zero byte;
- the model's name, the catalogue's revision, the adapter's name and the adapter's
  revision, in that order, each as bytes: the tag `b`, the length as eight bytes,
  big-endian, and the text's bytes;
- the count of files: the tag `u` and eight bytes, big-endian;
- for each file, in the order of the bytes of its name: its name (`b`, as above), its
  size (`u`), and its hash (the tag `d` and the 32 bytes of the digest).

It is written `sha256:` and 64 lower-case hex digits, and read back in that form and no
other (`identity::an_identity_is_written_one_way_and_read_strictly`).

**Three vectors,** computed apart from the code, which a reader can compute again from
the words above (`identity::the_identity_of_a_record_is_the_digest_the_adr_gives_for_it`).
A file's hash is the SHA-256 of the content given.

| Model | Catalogue's revision | Adapter | Adapter's revision | Files: name, size, content | Identity |
|---|---|---|---|---|---|
| `m` | `1` | `a` | `1` | `f`, 1, `x` | `sha256:9bdb763f4f3b18ce601572a1dce1142314a2cab53882897aebd70d357daf6a1f` |
| `tiny` | `2026-11.1` | `owlv2-sam2` | `7` | `LICENSE.txt`, 5, `terms`; `graph.onnx`, 3, `abc` | `sha256:9d10eec43a1b6f8abc3225115d33d98d2469e7bfd9c68c2c5ff428b50b5dd772` |
| `tiny` | `2026-11.1` | `owlv2-sam2` | `8` | the same two | `sha256:f04273f754765adcda8eac3dcfcc5f6d1a66bfe972276e0a8ac80897b9056ced` |

The first is 125 bytes, in hex:

```text
7768697a62616e673a696e7374616c6c6174696f6e2f3100
6200000000000000016d
62000000000000000131
62000000000000000161
62000000000000000131
750000000000000001
62000000000000000166
750000000000000001
642d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881
```

- **Every part of the record is part of the identity,** and each text is hashed with
  its length, so no two records share one
  (`identity::every_part_of_a_record_is_part_of_its_identity`). **Two records that
  differ in the adapter's revision alone have two identities**
  (`identity::two_records_that_differ_in_the_adapters_revision_alone_have_two_identities`):
  the second and third vectors.
- **Nothing local is in it:** no path, no time, no process, no address a file came
  from. The same entry installed under the same adapter's revision on two machines is
  one identity
  (`identity::an_identity_does_not_depend_on_the_order_the_files_were_given_in`,
  `identity::an_identity_read_back_from_a_record_s_bytes_is_the_identity_of_the_record_that_wrote_them`).
- **It is the version of a local model** in what a preparation saw (ADR 0004; the plan's
  task I7): a model's version is 1 to 128 printable ASCII characters (ADR 0006), and an
  identity's text is 71. So an installation replaced under a prepared call makes the
  preparation stale.
- The record is not hashed as its JSON. ADR 0001 turned that down for every identity:
  the bytes would depend on how a library writes JSON.

### The pointer

Code (task M2b: `install.rs`, `current`). `models/<model>/CURRENT` names the installation in
use. Its content is the identity's text and one line feed, 72 bytes, and it is read
back in that form and no other. It is replaced by writing a new file and renaming it
over the old one, under the directory's lock. It names a whole, verified installation
or it is not there: a model with no `CURRENT` is not installed, whatever else its
folder holds
(`publishing::an_install_of_two_files_ends_with_the_pointer_naming_a_verified_installation`).

**What is not as it is written is damage, never "not installed":** a pointer of
another length, with other white space, in upper case, that holds a secret the
reader was told of, that is a link or a folder; and a record that is not there, that
`InstallationRecord::parse` refuses, or whose identity is not the name of its folder.
The pointer and the record are each read through the store's streamed read, which
follows no link, and for no more than their bound and one byte
(`publishing::a_pointer_or_a_record_that_is_not_as_it_is_written_is_damage_and_never_not_installed`).
The reader does not read the installation's files.

**A reader that holds no lock** (the inventory, task M6) reads the pointer and then the
record it names. When the record is not there, it reads the pointer again before it
says anything: if the pointer now names another installation, or none, it answers from
that; it reports damage only when the pointer still names the installation whose
record is missing
(`publishing::a_reader_with_no_lock_that_finds_the_record_gone_reads_the_pointer_again`).
A pointer that names another installation at each of eight reads is not followed
further: the reader fails, and says neither damage nor "not installed".

### Downloading into staging

Code (task M2: `directory.rs`, `stage.rs`; tests named `staging::…`). `stage` downloads
each file of an entry through a `Downloader` (ADR 0012) into the model's one staging
folder, `models/<model>/staging/incoming/files/<file>`, and answers with what was
staged and the model's lock still held (`staging::two_files_are_staged_and_verified`).
An install stages through the same code under a lock it took itself, and publishes
("Publishing"); nothing publishes what `stage` hands back.

An install of an entry that needs a token is handed a credential with its downloader
(`Carrying`, `install_carrying`, `install_or_attach_carrying`; task N4). Each request
whose address is at the credential's origin carries it; each request for another
origin carries none, an address a file moved to included
(`credential::a_download_moved_to_another_origin_is_asked_there_with_no_credential`).
`stage`, `install` and `install_or_attach` carry none.

- **Under the model's lock,** taken without waiting before anything of the model is
  touched. Held by another, the answer is busy, and nothing is asked of the downloader
  or changed on the disk. One install of a model runs at a time, so one staging folder
  is enough; whoever has the lock removes what it finds there before it starts, and
  trusts none of it
  (`staging::staging_is_under_the_models_lock_and_the_next_cleans_what_the_last_left`).
- **A file is written as a stream** (ADR 0018), never held whole in memory: begun,
  appended to piece by piece, synced and finished, and the folder that names it synced.
  A file system that cannot stream is refused, not worked around
  (`staging::a_models_directory_on_a_file_system_that_streams_nothing_is_refused_and_not_worked_around`).
- **A download asks for one byte more than the file's stated size, and keeps no more
  than the size.** That is what the plan's "a download reads no more than the stated
  size" means here: no more is kept. The one byte asked beyond is how a file that is
  too long is told from one that was cut at its size, which a downloader that keeps to
  what it was asked would otherwise hand over as a whole file with the right hash. For
  a file of the right length the downloader hands over exactly `size`
  (`staging::a_download_asks_for_one_byte_beyond_the_stated_size_and_is_handed_exactly_the_size`).
- **The byte beyond is never written.** The sink counts what it keeps, and a piece
  that would take the file past its size is a file that is too long: none of the
  piece is written, and the sink answers no. The streamed file is also begun with the
  size as the most it may hold (ADR 0018), so the file system would refuse such a
  piece too. After any error nothing of a file is kept: its file is removed with the
  staging folder
  (`staging::a_file_one_byte_too_long_fails_the_download_and_leaves_no_staged_file_that_passed`).
  **What the file system refuses is the disk's refusal, whatever it calls it.** A
  full disk and a limit on a file's size both say that a file grew too large, of a
  file that is as long as its catalogue states; that is reported as the disk's, never
  as "longer than the catalogue states", and a downloader that then says the file
  moved is not followed
  (`staging::a_disk_that_lets_a_file_grow_no_further_is_a_disk_that_refused_and_not_a_file_too_long`,
  `staging::a_file_that_is_said_to_have_moved_after_the_disk_refused_a_piece_is_not_asked_for_again`).
  A downloader that goes on after its sink answered no has nothing more kept, and one
  that hands over bytes and then says the file moved is refused
  (`staging::a_downloader_that_goes_on_after_its_sink_answered_no_has_nothing_more_kept`,
  `staging::a_downloader_that_hands_over_bytes_and_then_says_the_file_moved_is_refused`).
- **A file is checked by its length and its hash, both:** fewer bytes than its size, a
  byte beyond it, and a SHA-256 that is not the entry's each fail the download. The
  hash is taken over the pieces as the sink keeps them, each whole, and a piece
  longer than the store takes at once (1 MiB) is written in parts
  (`staging::a_piece_is_hashed_whole_however_long_it_is`,
  `staging::a_piece_longer_than_the_store_takes_at_once_is_staged_whole_in_two_appends`).
  What the downloader says it handed over is not asked. Publishing reads the file
  back before it trusts it ("Publishing")
  (`staging::a_wrong_hash_fails_the_download_and_leaves_no_staged_file_that_passed`,
  `staging::a_short_file_fails_the_download_and_leaves_no_staged_file_that_passed`).
- **A file that moved** is asked for at the address it moved to only when
  `Entry::may_download_from` answers yes for that address's origin, and at most five
  times for one file (`MOVES_LIMIT`)
  (`staging::a_file_that_moved_is_asked_for_only_at_an_origin_the_entry_names`,
  `staging::a_file_that_moves_more_than_five_times_is_refused`).
- **A staging refused after it took the lock removes the staging folder and syncs
  the removal.** When the removal itself is refused, what is left is removed by the
  next to stage, which trusts none of it. A refusal before the lock touches nothing.
  No staged file that passed is left
  behind; which files passed is known only inside the staging that staged them
  (`staging::a_download_that_fails_or_is_cancelled_fails_the_staging_and_leaves_nothing_staged`,
  `staging::a_download_that_ends_as_cancelled_ends_the_staging_as_cancelled`).
- **A cancellation is asked before the lock is taken, before each file, at each
  piece, and after each answer of the downloader** (task M3), and at the last three
  the model's draining mark is read too (task M5d; "Removal"). The sink answers no
  to a piece that comes after a cancel, whether or not the downloader asks the
  cancellation itself; and when the downloader has answered, with bytes or with
  another address, a cancel ends the staging as cancelled before a move is followed
  or anything else is asked for
  (`attaching::a_cancel_is_asked_between_files_and_inside_one_whatever_the_downloader_does`,
  `attaching::a_cancel_before_a_move_is_followed_asks_for_nothing_more`).
- **Each step of the file system refused, and a crash before each,** with what was not
  durable lost each way the shim can lose it, and what was appended to a file and not
  synced left each way the shim can leave it: the staging fails, and the next staging
  of the model removes what was left and stages every file again, whole
  (`staging::each_step_of_the_file_system_refused_fails_the_staging_without_a_path_and_the_next_stages`,
  `staging::a_crash_at_any_event_of_staging_leaves_what_the_next_staging_removes_and_stages_whole`).
  Nothing of a crashed staging is ever taken up again, so nothing rests on what a
  crash leaves of a file.
- **A refusal names the file by its name in the catalogue,** as `FileName::shown`
  shows it, with fixed words and no path; a fault's own words, which are a server's,
  are not kept
  (`staging::a_refusal_names_the_file_by_its_catalogue_name_escaped_and_cut_with_no_path`).
- **The installer looks again for a secret,** with its own secrets, in the model's
  name and every file's, before it makes anything: an entry may have been made by a
  constructor that was told of others. With one found nothing is made, asked or
  written
  (`staging::a_name_that_holds_a_secret_the_installer_was_told_of_never_becomes_a_name_on_the_disk`).
- **What is in staging is not trusted, and nothing outside the directory is touched.**
  A link where the staging folder belongs is removed, not followed
  (`staging::a_link_where_staging_belongs_is_removed_and_never_followed`,
  `staging::staging_touches_nothing_outside_the_models_directory`).

The test kit's fake, `whizbang_testkit::downloads::FakeDownloader`, serves a few
kilobytes from memory in pieces and fetches nothing. It keeps to the most it is
asked for, or is told not to; answers that a file moved, and follows nothing; fails at
once or after a stated count of bytes; stops at a barrier after a stated count of
bytes and sees a cancellation there; counts a piece its sink answered no to; and keeps
every request with what it handed over
(`downloads::the_fake_serves_a_file_from_memory_in_pieces_in_order`,
`downloads::the_fake_hands_over_no_more_than_the_most_it_was_asked_for`,
`downloads::a_fake_told_to_serve_too_many_hands_over_everything_whatever_it_was_asked_for`,
`downloads::a_sinks_no_ends_the_download_with_that_piece_counted_and_nothing_after_it`,
`downloads::the_fake_answers_a_file_that_moved_with_its_address_and_asks_nothing_of_it`,
`downloads::the_fake_fails_at_once_or_after_a_stated_count_of_bytes`,
`downloads::the_fake_fails_a_request_for_an_address_it_serves_nothing_at_and_does_not_panic`,
`downloads::the_fake_stops_at_a_barrier_after_a_stated_count_of_bytes_until_it_is_released`,
`downloads::a_download_cancelled_while_the_fake_is_stopped_ends_as_cancelled_with_nothing_more_handed_over`,
`downloads::the_fake_keeps_each_request_in_order_with_what_it_asked_for_and_the_headers_it_carried`,
`downloads::told_twice_of_one_address_the_fake_does_what_it_was_told_last`,
`downloads::the_fake_may_be_asked_by_two_threads_at_once_and_counts_both`).

### Publishing

Code (task M2b: `install.rs`, `install`; tests named `publishing::…`). One call
installs a model: it takes the model's lock, stages the entry's files ("Downloading
into staging"), and publishes them. The adapter's revision is given by the caller,
who has it from the adapter the entry names; no adapter is in the build yet, so a
test states one. Under the model's lock, held from before anything of the model is
touched to the end
(`publishing::one_install_of_a_model_runs_at_a_time_and_the_other_is_busy_and_touches_nothing`):

1. **The status record is written first:** `installing`, no file staged. Then
   **what the pointer does not name is deleted,** as below, and a pointer being
   written that a dead publication left (`CURRENT.new`) is removed.
2. **An identity that is on the disk already and whole is not downloaded again.**
   The folder of the identity being installed, when there is one, is read against
   the record: its `RECORD` byte for byte, its files each by size and hash, and
   nothing else in it. Whole, it is what would be staged: the status says
   `installing` with every file staged, and the install goes to step 6 with it.
   Otherwise:
3. The files are staged, and **each staged file is read back from the disk** through
   the store's streamed read (ADR 0018), a piece of at most 1 MiB at a time, counted,
   and stopped one byte past its size. It is held to its size and its hash, both: a
   file of the right length with one byte another, a file a byte longer or shorter, a
   missing file and a link where the file belongs each end the install, and nothing
   is published
   (`publishing::a_staged_file_that_is_not_on_the_disk_as_it_was_downloaded_is_not_published`).
4. **A cancellation is asked for the last time here,** when the staged files have
   been read back. A cancel that arrives after the downloader handed over the last
   file and before this ends the install as cancelled; from the rename of step 5 on,
   the publication runs to its end and the answer is the installation
   (`publishing::a_cancel_before_the_staged_folder_is_renamed_ends_the_install_and_one_after_it_does_not`).
5. `RECORD` is written into the staging folder and the folder synced. The staged
   folder, with its `RECORD` and its `files/`, is renamed to
   `installations/<64 hex digits>` (the syncs around the rename are step 6's).
   Nothing names it yet.
6. Before the rename, the directory and `models/` are synced, which name `models/`
   and the model's folder: a folder that was found made was made by one who may
   have been refused the sync of its name; after it, `installations/`, for a folder
   renamed in now and for one found whole: whoever renamed that one may have been
   refused this sync
   (`publishing::an_install_answers_only_when_the_folders_that_name_the_model_are_durable`).
7. Under the directory's lock: the draining mark is read, and with one set nothing
   more is published (the install is refused; task M5d makes it stop sooner); the
   identity's hold file is made if it is not there, and `holds/` synced, whether the
   file was made now or found; the new pointer is written beside the old and renamed
   over it, and the model's folder synced. Nothing else is done under that lock: no
   download, no reading back, no deleting
   (`publishing::under_the_directorys_lock_a_publication_makes_the_hold_file_and_replaces_the_pointer_and_no_more`,
   `publishing::a_mark_of_draining_stops_the_publication_before_a_hold_file_or_a_pointer_is_made`,
   `publishing::a_hold_file_that_was_made_and_never_synced_is_synced_by_the_next_to_publish_its_identity`).
8. What the pointer does not name is deleted again, as below.

**Nothing is deleted, and no install is reported done, on the word of a name that is
not durable.** The deleting syncs the model's folder before it reads the pointer, and
deletes nothing when that sync is refused: a pointer whose rename a crash can still
undo may come back naming what was superseded. And an install that finds its
identity whole and named already goes through step 7 all the same, hold file, pointer
and both syncs, before it answers: the install that set that pointer may have been
refused the sync that makes it durable
(`publishing::the_pointers_sync_refused_and_then_a_crash_leaves_the_old_installation_or_the_new_one_whole`,
`publishing::asking_again_after_the_pointers_sync_was_refused_makes_the_pointer_durable_before_it_answers`).

A crash before the pointer's rename is durable leaves the model as it was: the pointer
names what it named, or nothing. A folder under `installations/` that the pointer does
not name is not an installation in use, and is cleaned as below.

- **A refused install leaves the pointer naming what it named;** a repair refused
  after it cleared the pointer leaves the model not installed. It removes its staging
  under the model's lock, and says how it ended in the status record
  (`publishing::a_failed_install_leaves_the_previous_installation_named_and_readable_and_its_status_says_how_it_ended`).
  Each step of the file system refused in turn leaves the model as it was, or, for a
  step after the pointer was made durable, installed, and the next install ends with
  the one installation, whole
  (`publishing::each_step_of_the_file_system_refused_leaves_a_first_install_not_installed_or_whole`,
  `publishing::each_step_of_the_file_system_refused_leaves_the_previous_installation_or_the_new_one_whole`).
  One step is neither ("Limits": the last sync). A refusal is fixed words, a file's
  name as it is shown, and no path
  (`publishing::every_refusal_of_an_install_says_one_sentence_of_fixed_words`). A
  staging folder that is on another file system, mounted at `staging/`, makes the
  rename fail: that is a refusal, not damage. (A link at `staging/` is refused
  before anything is staged: "A folder of the layout that is a link". Until that
  section this record said that such a link was followed, and named it as the way
  to this case.)
  (`publishing::a_staging_folder_on_another_file_system_is_a_refusal_and_not_damage`).
- **A publication refused after its folder was renamed in leaves the folder,** whole
  and not named: the mark was set, or the directory's lock was not let go, or a step
  of the file system was refused. What was downloaded is not thrown away; the next
  install of that identity finds it (step 2), and a removal, or an install of
  another identity, deletes it
  (`publishing::a_directory_whose_lock_is_not_let_go_refuses_the_publication_and_names_what_it_named`).
- **A record that would spell a secret the installer was told of is not made,** and
  then nothing is made, asked or written; nothing an install writes holds one
  (`publishing::a_record_that_would_spell_a_secret_is_not_installed_and_nothing_written_holds_one`).
- **The installation the pointer named before is superseded,** and publishing does not
  delete it: a run may hold it. An installation is superseded when it is under
  `installations/` and the pointer does not name it; there is no mark for it. **It is
  deleted by one routine,** which every install runs under the model's lock, at its
  start and at its end, and which every removal of the model runs (task
  M5c, built), over everything under `installations/` that the pointer does not name, but
  for a folder of the identity an install is installing: it
  tries the hold file alone, once,
  without waiting; with it, it deletes the folder and syncs; without, it leaves the
  folder for the next install or removal
  (`publishing::a_superseded_installation_is_deleted_when_no_hold_is_on_it_and_left_while_one_is`).
  No new hold can arrive on a superseded
  installation, since a hold is given only on what the pointer names. A folder with
  no hold file was never published: it is deleted, and no hold file is made for it;
  so is anything there that is no installation's folder, a link itself and not what
  it leads to. **With a pointer that cannot be read nothing is deleted,** since which
  installation is in use is then not known
  (`publishing::what_the_pointer_does_not_name_is_deleted_and_a_folder_with_no_hold_file_gets_none`).
  The routine reads the pointer without the directory's lock: a pointer is changed
  only by one who holds the model's lock, which the routine's caller does.
- **An identity that is on the disk already** (step 2): installed before and
  superseded while held, in use, or renamed in by an install that was refused before
  it pointed to it. Whole, nothing is downloaded and the pointer is set to it, again
  when it names it already. Finding it whole reads every file of the folder, by size
  and hash, under the model's lock: for a large model that is minutes of reading and
  no download (1C, with the first real model). Damaged and not in use, it is deleted
  when its hold file is free, and the staged folder renamed in; held, the install is
  refused as held (`InstallRefused::Held`)
  (`publishing::an_identity_that_is_on_the_disk_already_is_pointed_to_when_whole_and_replaced_when_damaged`).
- **A repair has the identity of what it repairs,** and so the same folder. Under the
  model's lock, the damaged installation is removed first, as a removal removes it:
  under the directory's lock the draining mark is read and the pointer is cleared;
  the hold file is tried alone; and the folder is deleted. Only then is the staged
  folder renamed in and the pointer set. **In between the model is not installed.**
  With a hold on it the repair is refused as held and the pointer is put back,
  whatever the mark says by then; with the mark set before the pointer is cleared, the repair is
  refused and nothing is changed
  (`publishing::a_repair_clears_the_pointer_deletes_the_damaged_installation_and_then_publishes`,
  `publishing::a_mark_of_draining_stops_the_publication_before_a_hold_file_or_a_pointer_is_made`).
  **The repair tries the hold file once.** Waiting for holds until a deadline needs
  the clock and the pacing of removal (task M5b), which may give the repair the same
  wait.

### The status record

Code (tasks M2b and M5b: `status.rs`; tests named `status::…`). `models/<model>/STATUS`
is what the holder of the model's lock last said: whether the model is being
installed or removed, which installation an install is of and how many of the
entry's files are staged, and how the last install or removal ended. It is compact
JSON under the format `status/2`, in one form, which is one of two shapes. Of an
install:

```json
{"format":"status/2","state":"installing","installation":"sha256:9d10eec43a1b6f8abc3225115d33d98d2469e7bfd9c68c2c5ff428b50b5dd772","files_done":1,"files":2}
```

and of a removal:

```json
{"format":"status/2","state":"removing"}
```

| Field | What it is |
|---|---|
| `state` | Of an install: `installing`; or how the last install ended: `installed`, `failed`, `cancelled`. Of a removal: `removing`; or `removed`, when the last removal cleared the pointer |
| `installation` | Of an install only: the identity of the installation it is of, as an identity is written |
| `files_done` | Of an install only: how many files were staged and held to their size and hash, when the record was written |
| `files` | Of an install only: how many files the entry has: 1 to 64 |

- **It is read strictly,** as an installation's record is ("How a record is read"):
  at most 256 bytes (`STATUS_MAX_BYTES`), refused over that before anything of it is
  looked at; an unknown field, a missing one, a value of another kind, a state that
  is none of the six, more files staged than there are, and any bytes but the one
  form are refused. A removal's record with a field of an install, an install's
  without its installation, and an identity that is not one as it is written are
  refused. It is read through the streamed read, so a link is not followed
  (`status::a_status_record_round_trips_in_one_form`,
  `status::a_status_of_an_install_names_its_installation_and_one_of_a_removal_names_none`,
  `status::a_status_counts_one_to_a_bounded_count_of_files_and_no_more_staged_than_there_are`,
  `status::a_status_record_is_read_strictly_and_refused_over_its_bound`,
  `publishing::the_status_says_installing_and_how_many_files_are_staged_while_the_install_goes`).
- **`status/1` is read no more.** It had no state for a removal and named no
  installation. No models directory that is kept holds one (the plan, "What undoing
  each task takes"): a record of that format is refused as another format, which a
  request that attached takes as no status, and the next install or removal writes
  over it.
- **It is written by the holder of the model's lock:** when an install starts, after
  each file is staged, and when the install ends, however it ends; when a removal
  has the lock, and when it ends. It is written whole beside the old one
  (`STATUS.new`) and renamed over it. An install that never had the lock (busy, a
  secret in a name, cancelled before it began) writes none.
- **Every install says `installing` as its first step under the lock** (task M3),
  before it deletes, reads or downloads anything, and so does one that finds its
  identity on the disk: reading it whole can take minutes, and until task M3 such
  an install wrote the final status only, so that one who looked meanwhile was
  shown how the install before it had ended. It says `installing` with every file
  staged before it points. So while an installer has the model's lock the status
  says `installing`, and a status that says anything else under a held lock is not
  an installer's
  (`attaching::an_install_says_installing_before_anything_else_whether_it_downloads_or_finds_its_identity_whole`).
  One case is not so: an install that is refused that first write ("Limits").
- **Every removal says `removing` as its first step under the lock** (task M5b),
  before it reads the pointer, and `removed` once it has taken the pointer away,
  whether or not the sync after that passed. **A removal that changed no pointer
  puts the status back:** the record it found, or none when it found none or one it
  could not read. So `removed` is said only of a model whose pointer a removal took
  away, and after a removal that answered busy the status says how the last install
  ended, as before it
  (`remove::a_removal_during_a_held_installation_refuses_a_new_hold_and_goes_on_when_the_hold_is_let_go`,
  `remove::with_the_hold_kept_past_the_deadline_the_removal_answers_busy_the_mark_is_gone_and_the_next_hold_is_given`,
  `remove::each_step_of_the_file_system_refused_leaves_the_model_installed_or_removed`).
- **After a refused sync of the pointer it says `failed` while the pointer names the
  new installation** ("Limits"): the install was refused, and it is the pointer that
  says what is installed. Asking again answers with the installation and says
  `installed`. One who attached to that install is told failed, as the installer
  was ("Attaching and cancelling").
- **No install, hold or removal is decided from it, and it is given no sync of its
  own.** Whether an install or a removal is running is said by the model's lock;
  which installation is in use is said by the pointer. One thing is answered from
  it: how an install ended, to a request that attached to it and never held the lock
  ("Attaching and cancelling"). A crash can leave a status that says `installing` or
  `removing` with nobody alive: a status that says so while the model's lock is free
  is the status of one that died. A status that cannot be written fails an install
  that has not published and a removal that has done no more than set its mark, and
  changes nothing of one that has decided.
- **It holds no name and no path:** a state's word, and of an install an identity
  and two counts.

### Attaching and cancelling

Code (task M3: `install.rs`, `install_or_attach`; `stage.rs`; tests named
`attaching::…`). `install_or_attach` is `install` when the model's lock is free. When
another has it, the request starts no install of its own and downloads nothing: it
attaches to whatever holds the lock, and ends as that ended
(`attaching::two_requests_in_one_process_cause_one_download_of_each_file_and_end_with_one_result`,
`attaching::a_second_process_that_asks_while_the_first_is_stopped_sees_progress_and_ends_with_the_same_installation`,
`attaching::a_request_that_finds_no_install_running_installs_and_one_already_cancelled_does_nothing`).

- **It tries; it does not wait.** The plan and the first form of this record had
  the attached request wait on the model's lock. A request blocked in the file
  system on a lock can show no progress and cannot leave: it sees neither the
  status record nor its own cancellation until the install is over. So it tries
  for a share of the model's lock (`try_share`, ADR 0018), which answers at once,
  and tries again. **Between two tries it calls `watch`, which its caller gave it,
  once,** with what the status record says when that is `installing`, and with
  nothing when it says anything else or cannot be read. `watch` is where progress
  is shown, and it is what paces the tries: the caller waits there as long as it
  chooses. One who attaches does not sleep here, and does not try twice without
  that call in between, so it does not spin by itself (the one place in the
  library where anything waits by itself is the directory's lock, since task N2:
  "Three kinds of lock"); a caller whose `watch`
  returns at once tries as fast as the file system answers, which is what the
  tests do, giving way to other threads. The pause a command makes there is the
  command's: `whizbang models get` pauses a quarter of a second of its clock at each
  call, and says what is shown only when it changes (task N3b;
  `models::a_second_get_attaches_to_the_first_waits_by_the_clock_and_ends_with_the_same_installation`).
  Its waiting has no limit of its own: the install it waits for ends by its own
  limits, or its process ends and the share is given.
- **It ends by what the status record says,** read once it has its share, when
  nobody can be installing or removing. `installed`, with the pointer naming the
  installation of the entry it asked for: that installation, read by `current`.
  `installed` with a pointer that names another installation: busy, since what ran
  was an install of another revision; asked again, it installs
  (`attaching::a_request_that_attached_to_an_install_of_another_identity_is_busy`).
  `cancelled`: cancelled. `failed`: failed (`InstallRefused::Failed`), with no
  reason, which only the installer has
  (`attaching::a_request_that_attaches_ends_as_the_install_ended_failed_or_cancelled`).
  **Only of an install of what it asked for** (task M5b): the status names the
  installation the install was of, and one that was cancelled, failed or died as an
  install of another identity is busy, as one that was installed is; until `status/2`
  such a request was told failed or cancelled of an identity nobody had tried to
  install for it. **And `removing` or `removed` is busy:** what had the lock was a
  removal, and the request is never told installed across one that cleared the
  pointer
  (`attaching::an_attached_request_is_answered_by_the_status_of_an_install_of_what_it_asked_for_and_is_busy_after_any_other`,
  `remove::one_who_attaches_to_a_removal_is_answered_busy_and_to_one_that_changed_nothing_as_before`).
  A status that still says `installing` with the lock free is an install whose
  process is gone: failed, and the pointer names what it named
  (`attaching::when_the_installing_process_is_killed_the_attached_one_reports_the_install_as_failed`).
  No status, or one that cannot be read: failed, with the pointer naming the
  installation that was asked for as with any other
  (`attaching::a_request_that_attached_and_finds_no_status_or_one_it_cannot_read_is_told_failed`).
- **Never by the pointer alone.** After a refused sync of the pointer ("Limits") the
  status says `failed` over a pointer that names the new installation, which no
  sync has made durable. The attached request is told failed, as the installer was:
  no request is told that a model is installed on the word of a name that is not
  durable. Asked again, it finds the identity whole, downloads nothing, makes the
  pointer durable and answers with the installation
  (`attaching::an_attached_request_answers_from_the_status_and_never_from_a_pointer_alone`).
  A status that says `installed` was written by an install whose last sync
  passed, and every install that takes the lock after it says `installing` before
  it touches the pointer.
- **What is shown between tries is an install or nothing.** Under a lock held by one
  who is not installing, the status says `removing`, or how the last install ended,
  and is not shown as progress
  (`attaching::while_the_lock_is_held_and_the_status_says_no_install_is_running_nothing_is_shown`).
- **An install is cancelled by the request that started it,** and by no other: its
  cancellation is asked before the lock is taken, before each file, at each piece,
  after each answer of the downloader, and for the last time when the staged files
  have been read back ("Downloading into staging"; "Publishing", step 4). A cancel
  while the download is stopped inside a file ends the install as cancelled,
  removes its staging and leaves the previous installation named, with nothing
  asked or handed over after it
  (`attaching::a_cancel_while_the_download_is_stopped_inside_a_file_ends_the_install_as_cancelled`).
  A cancel after the pointer was replaced changes nothing: the answer is the
  installation
  (`attaching::a_cancel_after_the_pointer_was_replaced_returns_the_installed_result`).
- **An attached request that goes away cancels nothing.** Its cancellation is its
  own: asked before each try, it ends the request as `InstallRefused::Left`, and
  the install goes on to its end. So does an attached process that is killed
  (`attaching::an_attached_request_that_goes_away_does_not_cancel_the_install`).
  A request that was cancelled before it asked is cancelled, and waits for nothing.

### Holds

Code (task M5: `hold.rs`; tests named `holds::…`). **A hold is a file lock, not a
process id.** A hold on one installation is a shared lock on its hold file,
`holds/<64 hex digits>`, taken under the directory's lock after the draining mark and
the pointer were read there: it is given when no mark is set whose remover is alive
("Removal": "A mark is live only while its remover is") and the pointer names
that installation, and it makes nothing on the disk but the directory's `LOCK`,
which is made when it is taken and is not there: a hold asked of a directory where
nothing was ever installed makes that one file, and is refused
(`holds::a_hold_is_taken_under_the_directorys_lock_and_makes_nothing`). While it is
there, nobody takes the hold file alone, so the installation's folder is not deleted:
superseded, it stays until the hold is let go
(`holds::a_hold_keeps_a_superseded_installation_on_the_disk_until_it_is_let_go`).

**A hold is given or refused at once:** its share of the hold file is a try that does
not wait (ADR 0018, `try_share`), and nothing is waited for but the directory's lock.
So where the plan speaks of a waiting hold, here there is a hold that was refused and
may be asked for again. What a refusal is (`HoldRefused`), each fixed words and no
path:

| Refused as | When |
|---|---|
| `Draining` | The mark is set, and its remover is alive (task P5a of 1C: until then, the mark alone). It is read before the pointer, so a model whose pointer a removal has cleared is draining, not "not named" (`holds::a_hold_is_refused_while_the_mark_of_draining_is_set_whatever_the_count`) |
| `NotNamed` | The pointer names another installation, or there is none: an installation that was superseded, one never installed, a model never installed (`holds::a_hold_on_an_identity_the_pointer_no_longer_names_is_refused`) |
| `Damaged` | The pointer is not a pointer as it is written, or the installation it names has no hold file. A hold never makes one (`holds::a_pointer_that_cannot_be_read_or_a_missing_hold_file_is_damage_and_a_hold_makes_none`). Or the model's count of removals is not a count as it is written (task P5a of 1C; `holds::a_hold_returns_the_count_of_removals_it_read_under_the_directorys_lock`) |
| `Deleting` | The hold file is held alone. That is an install that deletes the installation's folder, which is only for an installation the pointer does not name; or a removal, the first to take a hold file alone while the pointer names its installation, whose mark refuses the hold as `Draining` before the hold file is tried (`remove::a_removal_marks_under_the_directorys_lock_says_removing_and_clears_the_pointer_with_every_hold_file_its_own`) |
| `DirectoryBusy` | The directory's lock was not let go ("Three kinds of lock") |
| `Disk` | The file system refused a step, the try among them: a file system that answers that it cannot try has not said "held alone" (`holds::a_hold_file_held_alone_is_a_hold_refused_at_once_and_a_try_that_fails_is_an_error`, which holds the two rows above too) |
| `NotAnIdentity` | Of a hold asked through the inventory (`Installations::hold`, "The lease"): the identity that was asked for cannot be read back as an installation's, because its text holds a secret the inventory was told of. Until the review of task M6b's first pull request this was answered as `HoldsSecret`, whose words are of a model's name (`leases::a_hold_is_asked_by_a_declared_name_that_is_looked_up_in_the_catalogue`) |
| `HoldsSecret` | The model's name holds a secret the holder was told of. Nothing is opened by it (`holds::a_model_name_that_holds_a_secret_opens_nothing_and_no_refusal_shows_a_path`) |

The operating system lets a lock go when its process ends, however it ends, so a hold
of a dead process is no hold; nothing reads a process id from a file, and nothing is
signalled by one (rascal's R3 and R5). A second process that holds an installation
and is killed leaves the hold file free to be taken alone, and the next to take the
model's lock deletes the superseded folder
(`holds::a_child_process_that_holds_a_model_and_is_killed_leaves_no_hold`).

**A hold stops a repair.** A repair of the installation in use deletes its folder,
so with a hold on it the repair is refused as held, the folder is kept and the
pointer names it still; it passes once the hold is gone
(`holds::a_hold_stops_a_repair_of_the_installation_in_use_until_it_is_let_go`).

**Holds are counted inside a process** as well (`Holds`, which a process makes once
for a directory and takes every hold through). A process's own shared lock on a hold
file stops its own try to take the file alone, as another process's would, and its
second share of the file is a second lock; so a process keeps one share of a hold
file and a count of its holds on it, and lets the share go when the count is nothing.
Two holds in one process and one in a second process are all given, with one try at
a share in the first
(`holds::two_holds_in_one_process_and_one_in_a_real_second_process_are_all_given`).

- **The count does not stand in for the check.** Every hold reads the mark and the
  pointer under the directory's lock, whatever the count: with one hold counted on
  an installation that was superseded, or whose model is draining, the next is
  refused, and the one that was given stays (the tests of `NotNamed` and of
  `Draining`, above).
- **A hold let go twice is let go once** (`Hold::let_go`, and the drop): the count
  goes down by one, whatever is called after
  (`holds::a_hold_let_go_twice_is_let_go_once`).
- **Letting the last hold go and taking a new one go through one guard** inside the
  process, so that the share is not let go under a hold that was just counted
  (`holds::the_last_hold_let_go_and_a_new_one_taken_by_two_threads_never_leave_a_hold_without_its_share`).
- **Two sets of holds over one directory in one process** are as two processes are:
  each has a share of its own. Nothing is wrong then but an open file more.
- **A hold does not read the installation.** It is given on what the pointer names,
  whether or not the folder is whole; that is the publication's and the inventory's
  to say (task M6).
- **A hold has the model's count of removals as it read it** (`Hold::removals`;
  task P5a of 1C), under the directory's lock, after the mark and the pointer and
  before its share of the hold file: "Removal", "A mark is live only while its
  remover is, and the count of removals".

### Removal

Code up to the decision, inside one process (task M5b: `remove.rs`, `remove`; tests
named `remove::…`), and against an install and across processes (task M5d, below).
and the deleting (task M5c). In this order:

1. **The remover's lock, and then the draining mark.** The removal takes the
   remover's lock alone, by trying until its deadline; past it, another removal of
   the model being alive, it answers busy and has set no mark (task P5a of 1C,
   below: until then there was no such lock, and the mark came first). The mark
   is set under the directory's lock, before the removal has the model's lock,
   and under the same lock the model's count of removals is raised. What was at
   the mark's name was left by a remover that died, and is replaced. From then no
   hold on that model is given, and no install of it publishes. A model that
   has no folder was never installed: it is not marked, and nothing is made.
2. The removal takes the model's lock alone, by trying until its deadline. Past it
   the removal answers busy (`RemoveRefused::Busy`) and takes its mark off (6).
   With the lock, what is at the mark's name and is no mark as whizbang writes
   one is replaced.
3. **It says `removing`** in the status record ("The status record"), and syncs the
   model's folder before it reads the pointer: nothing is decided on the word of a
   pointer that is not durable. **A model with no pointer is not installed,** and
   the removal says so (`RemoveRefused::NotInstalled`)
   (`remove::a_removal_of_a_model_that_is_not_installed_says_so`). Before it
   answers it deletes what no pointer names and removes the staging an install
   left ("On the disk";
   `remove::a_removal_that_finds_no_pointer_removes_the_staging_a_dead_install_left`).
4. **The holds are waited for by trying, not by waiting:** the hold file of each
   installation of the model is tried alone, without waiting, again and again until
   the deadline, and kept once it is given. The deadline is `patience` from the
   start, of the injected clock (`Clock::now`). Between two tries, of the remover's
   lock, of the model's lock and of a hold file, nothing sleeps and nothing reads another clock: what
   comes there is one call of `pause`, which the caller gives, and a test moves
   the clock by hand there. The directory's lock is another matter since task N2:
   a removal asks for it as everyone does, and each asking paces its own tries by
   the clock the directory was opened with, for five seconds at most ("Three
   kinds of lock"; "Limits" has the sum). On a clock that stands still a removal tries for as long as
   the hold is kept.
5. **With every hold file its own, the removal clears the pointer,** under the
   directory's lock, and syncs the model's folder. That is the decision: the model
   is not installed. It says `removed`. **Then it deletes** (task M5c): everything
   under `installations/`, each by its name as the listing gave it, a link itself
   and not what it leads to; it syncs `installations/`, removes the staging folder
   an install left when there is one, and only then lets the hold files go. **The
   hold files are kept,** as every lock file is: one empty file for each identity
   of the model that was ever published, under `holds/`, after its installation is
   gone. They are not deleted once the folder is gone, though a removal has each
   alone then. A lock is on the file and not on its name ("Three kinds of lock"):
   a hold file removed while anyone has it open, or is about to open it by its
   name, leaves that one with a lock on a file that has no name, beside the next
   to make a file at that name. For a removal the deleting could be argued safe
   as the code is now: whoever opens a hold file has the model's lock, which the
   removal has, or has read under the directory's lock a pointer that names its
   installation, which the removal has cleared. But the argument would have to be
   kept true of every later opener, and the rule that no lock file is ever
   removed needs no argument. It costs one directory entry for each identity,
   and an installation of the same identity made later uses the same file
   (`remove::a_removal_deletes_every_installation_of_the_model_under_its_hold_files_and_touches_nothing_outside`,
   which finds both hold files there and free after the removal).
6. **Its own mark is taken off at the end, however the removal ended,** under the
   directory's lock, and the model's folder synced; then the remover's lock is
   let go. **A mark is a removal's own** when the removal has the remover's lock
   (task P5a of 1C; until then, when it made the mark where none was or had the
   model's lock). A removal that is not given the remover's lock sets no mark and
   takes none off: the mark that is there is the mark of the removal that has
   that lock. The mark of a remover that died is nobody's, and the next removal
   makes it its own
   (`remove::a_second_removal_that_is_busy_leaves_the_mark_of_the_one_that_has_the_lock_and_no_hold_is_given_meanwhile`,
   `remove::a_removal_that_made_the_mark_and_never_had_the_lock_takes_its_own_mark_off_and_no_other`,
   `remove::a_removal_that_made_its_mark_again_while_it_tried_for_the_lock_takes_it_off_when_it_ends_busy`). **Past the deadline the removal
   answers busy** (`RemoveRefused::Held`): it has changed no pointer, it lets go the
   hold files it had, puts the status back and takes the mark off, and the next hold
   is given.

(`remove::a_removal_during_a_held_installation_refuses_a_new_hold_and_goes_on_when_the_hold_is_let_go`,
`remove::with_the_hold_kept_past_the_deadline_the_removal_answers_busy_the_mark_is_gone_and_the_next_hold_is_given`,
`remove::a_removal_marks_under_the_directorys_lock_says_removing_and_clears_the_pointer_with_every_hold_file_its_own`.)

- **The pointer is cleared after the holds are let go, not before** (changed with
  task M5b). The first form of this record cleared the pointer and then waited, and
  on its busy path put the pointer back before it took the mark off. The mark
  alone refuses every new hold, so nothing is gained by clearing first; and
  clearing last means that a removal that answers busy has changed no pointer and
  has none to put back, which a refused step could have left cleared; that a crash
  while a removal waits leaves the model installed, with a stale mark, and never
  "not installed" under a run that still holds it; and that a reader never finds a
  model without a pointer that is then given one back. It is also what the plan's
  task says: the holds are waited for, "then the removal goes on", and "goes on"
  is the pointer cleared.
- **Which holds.** Those on every installation of the model that is on the disk:
  the one the pointer names, whether or not its folder is there, and any that was
  superseded and is still held by a run. A model is not removed under a run that
  uses an older installation of it
  (`remove::a_removal_waits_for_a_hold_on_a_superseded_installation_too`,
  `remove::a_removal_waits_for_a_hold_on_the_installation_the_pointer_names_though_its_folder_is_gone`).
  A folder with no hold file was never published and is held by nobody.
- **A waiting hold.** A hold is given or refused at once ("Holds"), so what the
  plan calls a waiting hold is one that was refused and is asked for again by its
  caller. A removal cancels it by the mark: asked again while the mark is set, it
  is refused as draining, by its own process and by any other, each time.
- **A pointer that cannot be read** does not stop a removal: which installation was
  in use is not known, every hold file is waited for, and what is at the pointer's
  name is taken away
  (`remove::a_removal_of_a_model_whose_pointer_cannot_be_read_clears_it`).
- **A removal and an install do not go on beside each other.** Each needs the
  model's lock. An install that is asked for while a removal has it is busy and
  touches nothing; a removal that finds an install running has changed nothing but
  the mark when its deadline comes, and takes it off
  (`remove::a_removal_and_an_install_in_one_process_do_not_go_on_beside_each_other`).
  And an install that is downloading stops for the mark (below).
- **Each step refused** (the removal has some twenty events): the model is
  installed, whole, with its status as it was, or it is not installed; a removal
  that answered has made that durable; and the next removal ends with the model
  not installed, no mark and nothing left of it
  (`remove::each_step_of_the_file_system_refused_leaves_the_model_installed_or_removed`).
  The crashes are under "On the disk", below.
- **A refusal is fixed words and no path,** and a model's name that holds a secret
  the remover was told of is refused before anything is touched
  (`remove::a_model_name_that_holds_a_secret_is_not_removed_and_no_refusal_shows_a_path`).

**The mark** is `models/<model>/DRAINING`, a record under the format `draining/1`
in one form, 23 bytes:

```json
{"format":"draining/1"}
```

It says that a removal of the model has begun, and nothing else: it names no
process and no time, and whether its remover is alive is not read from it (rascal's
R3): that is asked of the remover's lock (task P5a of 1C, below). It is at most 64 bytes (`DRAINING_MAX_BYTES`) and is read strictly, as every
record here is: refused over its bound before anything of it is looked at, with an
unknown field, in another format, and in any bytes but the one form
(`remove::the_mark_is_a_record_read_strictly_and_bounded`). **Everybody asks one
question of it, `draining`** (task P5a of 1C, below; until then a reader and one
who decides read it two ways, and neither asked whether its remover was alive):
no mark, or one whose remover is gone, is no; a mark under a remover that is alive
is yes; and what is at the mark's name under a live remover and is no mark is a
refusal to one who holds no lock (a command that lists models, 1C), and stops a
hold and a publication as a mark does, a link and a folder included, so that a
live remover's mark that cannot be read still stops them; and a removal replaces
whatever is there with its own
(`remove::a_mark_on_the_disk_is_read_strictly_by_a_reader_and_as_there_by_whoever_decides_by_it`).
The mark is given no sync when it is set: a crash that loses it leaves the model as
its pointer says.

**Against an install, and across processes** (task M5d). Nothing here reads a
process id, and nothing is signalled (rascal's R3): a removal reaches an install in
another process by the mark alone.

- **An install stops for the mark.** Once it has the model's lock, an install reads
  the mark wherever it asks its cancellation: before each file, at each piece, and
  after each answer of the downloader. With one set it ends as
  `InstallRefused::Draining`, removes its staging, says `failed`, and lets the
  model's lock go; the piece that came after the mark is not kept, whatever the
  downloader does. It leaves the mark, which is the remover's. The mark is read
  there as there or not there, without the directory's lock: stopping is always
  safe, and what a publication decides from is read under that lock, at its end
  (`remove::an_install_stops_for_a_mark_before_each_file_and_at_each_piece_whatever_the_downloader_does`).
- **A removal of a model that is installing cancels the install,** in its own
  process or another, and then goes on, of what was installed before: that is
  removed, and nothing of the install is left. When the removal's deadline comes
  first it is busy, what was installed before is kept, named and whole, and the
  install goes on. Never half of either
  (`remove::a_removal_of_a_model_that_is_installing_cancels_the_install_and_what_was_installed_before_is_removed_or_kept`,
  `remove::a_removal_cancels_an_install_that_runs_in_a_second_process`).
- **What "does not go on beside it" means** (the plan's task M5d left it to this
  task). With an install stopped after it took the model's lock and read the
  pointer, a removal sets its mark and tries for the lock: it changes nothing else,
  and at its deadline it is busy and takes the mark off; if the install is let go
  sooner, the install stops for the mark and the removal goes on. **A third outcome
  is that neither happened:** the install reads the mark and stops, and the
  removal's deadline comes before it is given the lock, so it ends busy too. The
  model is as it was; both are asked for again. With a removal
  stopped there, an install is busy and touches nothing, and so is a second
  removal. So: busy, or stopped by the mark; in no case do two act on the model
  (`remove::with_an_install_stopped_after_it_read_the_pointer_a_removal_does_not_go_on_beside_it`,
  `remove::with_a_removal_stopped_after_it_read_the_pointer_an_install_is_busy_and_touches_nothing`).
  Each stopped before its first step and let go together, they end in one of the
  two orders: the install whole and then removed; or the removal of what was
  there, and the install after it whole, or stopped for the mark, or busy
  (`remove::a_removal_and_an_install_let_go_together_end_in_one_of_the_two_orders`).
- **A mark left by a process that died is taken off by the next to take the model's
  lock,** and stops nobody meanwhile (task P5a of 1C, below). An install, as its
  first change after it said `installing`, takes off a mark it finds whose
  remover is gone, under the directory's lock; the mark of a remover that is
  alive it leaves, and stops for. The sync of the model's folder that follows is
  the one every install makes before it deletes anything; a mark that a crash
  brings back is a dead remover's again. Until task P5a an install could not
  tell a dead remover's mark from that of one that is alive and trying for the
  lock, and took off both. **A removal still sets its mark again** when it is
  gone, with the count raised: each time it is not given the model's lock, once
  it has it, and each time a hold file is not given it. Nothing in whizbang
  takes a live remover's mark off any more; a person can. Of two removals the
  second waits for the remover's lock: when the first ends busy it takes off
  the mark it made, and the second sets its own at its next try
  (`remove::a_removal_that_waits_for_a_hold_sets_its_mark_again_when_it_is_gone`,
  `remove::of_two_removals_the_one_that_made_the_mark_and_ends_busy_takes_it_off_and_the_one_with_the_lock_sets_it_again`)
  (`remove::a_mark_left_by_a_process_that_died_is_taken_off_by_the_next_to_take_the_lock`,
  `remove::a_removal_that_is_not_given_the_models_lock_sets_its_mark_again_and_again_when_it_is`).
- **The two cases of draining with the hold in a real second process:** kept past
  the deadline, busy; let go, the removal goes on
  (`remove::the_two_cases_of_a_removal_with_the_hold_in_a_real_second_process`).
- **Limits.** Between a mark being taken off, by an install or by a second removal
  that made it, and the remover setting it again at its next try, a hold can be
  given; the remover then waits for it as for any other. So a removal can be kept
  waiting to its deadline by holds that arrive in those moments; it never deletes
  under one. *An install that stopped for a mark says `failed`* in the status
  record, as one whose download broke does: a reader cannot tell the two apart,
  and one who attached to it is told failed. `status/2` was not given a state for
  it. *The mark is read by a call of the file system at every piece* of a
  download, a megabyte at most: one look for a name, no lock, and with a mark
  there one try for a share of the remover's lock. *After a remover
  that died,* its mark stays on the disk until someone installs or removes the
  model, and stops nobody (task P5a of 1C; until then every hold was refused as
  draining, which
  `remove::a_crash_at_any_event_of_a_removal_leaves_the_model_installed_or_not_installed`
  pinned as a limit and now holds the other way).
  *A removal is ended by its deadline and by nothing else:* it takes no
  cancellation, and needs none: the patience of `whizbang models remove --wait`
  bounds it, and a remover that is interrupted or killed no longer strands the
  model (the plan of 1C, decision D6). An install that is reading back what it staged, or reading an identity it
  found on the disk, asks neither its cancellation nor the mark until it is done:
  the mark stops it at the publication, under the directory's lock. And an
  install that stopped for a mark whose remover then died is not started again by
  anything here: it is asked for again. *An install that lands at a removal's
  end* (stated by task P5b, from the review of task P5a): a removal lets the
  model's lock go when it has drained, and only then takes its mark off and lets
  the remover's lock go (step 6). An install that takes the model's lock in that
  moment finds a mark whose remover is alive, at its first look where it asks its
  cancellation, and ends as `InstallRefused::Draining`, as it would a moment
  before; asked again once the mark is off, it goes on.

**A mark is live only while its remover is, and the count of removals** (task P5a
of checkpoint 1C; the plan's decisions D6 and D5). Code: `remove.rs` (`draining`,
`removals`, the remover's lock in `remove`), `hold.rs` (`Hold::removals`), and the
three who decide by the mark (`hold.rs`, `install.rs`, `stage.rs`). Nothing here
reads a process id, a time or a host name, and nothing is signalled.

- **The remover's lock** is `models/<model>/REMOVER`, a lock file as the model's
  `LOCK` is: made when it is first taken, empty, and never removed or replaced. A
  removal takes it alone, by a try, as the first thing it does to a model that has
  a folder, and keeps it until its mark is taken off, however the removal ends. Not
  given it, the removal calls `pause` and tries again until its deadline, then
  answers `RemoveRefused::Busy`: it has set no mark, raised no count and changed
  nothing. The operating system lets the lock go when the remover's process ends,
  however it ends. Its place in the order of locks is the first ("Three kinds of
  lock"): a removal takes it holding nothing.
- **`models::draining` is the one question,** and it answers yes only for a mark
  whose remover has that lock. Nothing at the mark's name: no, and no lock is
  tried. Something there: a try for a **share** of the remover's lock, which does
  not wait and is let go at once. Given the share, or with no lock file at all,
  no remover is alive and the answer is no, whatever is at the mark's name: a mark
  as it is written, an empty file, a folder. Refused the share, a remover is
  alive, and the mark is read strictly: a mark is yes, and anything else is a
  refusal. **A share, and not the lock alone,** because the remover has it alone:
  two that ask at the same moment are each given a share, and neither reads the
  other as a remover that is alive
  (`remove::two_that_ask_at_once_whether_a_dead_removers_mark_is_live_both_answer_no`,
  in which the second asks while the first has its share, both ways round).
- **Who asks it, and no other question:** a hold, under the directory's lock; a
  publication, where it points and where a repair clears the pointer, under the
  directory's lock; an install's own look at the mark where it asks its
  cancellation, and the look by which it takes a dead remover's mark off; and one
  who holds no lock and lists models (`models::draining` itself; the command is
  task N3b's). The supervisor of the worker asks it too, of a queued request whose
  count has not moved (task P5b; ADR 0020, "The worker", "Removal"). The three who decide go through one function (`remove::stops`): yes and a
  refusal both stop them, so a live remover's mark that cannot be read stops
  them as before; a file system that refused is an error and not a yes
  (`remove::a_hold_that_meets_the_mark_of_a_remover_that_was_killed_is_given`, of a
  real process that is killed with its mark set, and of a disk that loses power;
  `remove::a_hold_that_meets_the_mark_of_a_live_remover_is_refused_as_draining`;
  `remove::a_publication_an_installs_own_look_and_a_listing_take_the_mark_of_a_dead_remover_as_no_mark`).
- **The count of removals** is `models/<model>/REMOVALS`, a record under the
  format `removals/1` in one form, 64 bytes at most (`REMOVALS_MAX_BYTES`), read
  strictly as every record here is:

  ```json
  {"format":"removals/1","count":3}
  ```

  **Where it is written.** Every time a removal writes its mark, under the same
  holding of the directory's lock, it raises the count by one: the first time,
  and each time it sets the mark again. The new count is written whole to
  `REMOVALS.new` and renamed over `REMOVALS`. Nothing else writes it and nothing
  lowers it: not the end of the removal, however it ended, not an install, and
  not the removal of the model, whose folder and count stay. No file is no
  removal yet: nothing. **How it is read.** By `models::removals`, by one who
  holds no lock: the rename means a reader finds the old count or the new, never
  part of one. What is at its name and is no count is refused, with no path
  (`remove::each_removal_that_sets_its_mark_raises_the_count_and_a_removal_that_timed_out_leaves_it_raised`).
- **A hold returns the count it read** (`Hold::removals`), under the directory's
  lock, after the mark and the pointer and before its share of the hold file. A
  hold is given only while no live mark is set, and a mark is set only with the
  count raised under that lock: so a count that has moved from the one a hold has
  says that a removal of the model set its mark after the hold was given, whether
  or not the mark is still there. A removal that began and timed out between two
  looks at the mark is seen by the count. Inside a process, where holds on one
  installation are counted on one share, each hold has its own reading. A count
  that cannot be read as one refuses the hold as `Damaged`: a hold with no count
  could not be told from one that was never removed under
  (`holds::a_hold_returns_the_count_of_removals_it_read_under_the_directorys_lock`).
- **The crashes of a removal hold as they did**
  (`remove::a_crash_at_any_event_of_a_removal_leaves_the_model_installed_or_not_installed`,
  which now puts a crash at the taking of the remover's lock and at each event of
  the count as well, and after each finds that a hold on a model that is still
  installed is given). The count is given no sync of its own: the removal's sync
  of the model's folder makes it durable with the rest, and a crash before that
  can lose it or bring an older one back. That is a count lowered by a loss of
  power, which ends every process that read the lost one, so no hold that is
  still held has a count the crash made equal again.
- **Limits.** *A removal with no patience that meets a share:* one who asks
  whether a dead remover's mark is live has a share of the remover's lock for a
  moment, and a removal that tries for the lock in that moment and has no
  patience answers busy, though no other removal is alive. Asked again, it goes
  on. *A count that cannot be read* is counted from nothing again by the next
  removal that sets its mark: a hold given before the damage can then have a
  count equal to a later one, and a removal is not seen by it; the removal
  still waits for that hold, as for any other, and deletes nothing under it.
  Its owner is task P5b of checkpoint 1C, which leaves it as this line and says
  what follows from it: from the damage until the next removal of the model sets
  its mark, every hold on the model is refused as `Damaged`, so the model takes
  no new request; and every request queued for it in the worker's queue, which
  reads the count, is ended there, never sent, at its next read (ADR 0020, "The
  worker", "Removal"). The next removal, whether it ends well or busy, writes a
  count over the damage, and the model takes requests again.
  *A mark with no count raised* is not written by whizbang; one written by hand,
  under a remover's lock held by hand, stops holds and moves no count.

**For the tasks that follow.** *P5b* (built): the supervisor is told of a removal in its
own process, and for a removal in another reads `models::removals` for each queued
request and compares it with the count that request's hold returned; it asks
`models::draining` and no other question of a mark. A removal tells its caller each
time it sets its mark through `models::remove_and_tell` (`remove` is the same with
no one told), whose caller tells the supervisor; what the supervisor does with each
is ADR 0020, "The worker", "Removal". The remover keeps its lock until its mark is
off, however it ends, so that the mark is live to its last moment
(`remove::the_remover_keeps_its_lock_until_its_mark_is_off`, which stops a removal
just before it takes its mark off, at its end, and finds a share of the remover's
lock refused there; from the review of task P5a). *P6b:* what carries a hold's
count to the queue is not built here. `Hold::removals` is where it starts; the
lease (`inventory.rs`, `Leased`) has the hold and hands nothing on yet. ADR 0015
must be amended by that task, in "A wrapper around each request" and in "An
inventory" where it describes the lease's place: what a wrapper's `before` gives
back (`Open`) or what a backend is given for one request gains the count the
lease's hold read, so that a request enters the queue with it.

**On the disk** (task M5c).

- **Nothing is deleted before the cleared pointer is durable,** and nothing while a
  hold file is another's: the deleting follows the sync of the model's folder, and
  the hold files of every folder that is deleted are the removal's alone until the
  deleting is synced. So a crash leaves the installation whole and named, or the
  model not installed: never a pointer to files that are gone. **Nothing is
  deleted through a link.** A link under `installations/` that leads out of the
  directory is deleted itself, and what it leads to is as it was, shown on the
  list of events and by what is outside afterwards
  (`remove::a_removal_deletes_every_installation_of_the_model_under_its_hold_files_and_touches_nothing_outside`).
  A folder of the layout that is itself a link is refused, and nothing is listed
  or deleted through it ("A folder of the layout that is a link"). Until the
  review of this task's pull request this record said here that nothing outside
  the models directory is touched, on the word of the list of events. That was
  false of such a folder: with `installations/` a link, a removal listed what
  the link led to and deleted each name there, and answered that it had
  removed the model; the list could not show it, since every path on it is
  spelled inside the directory. What is left undefended is one moment, stated
  in that section.
- **A removal interrupted after the pointer was cleared is finished by the next
  install or removal.** It died or was refused with no pointer and the folder
  there, with its mark or without. The next install takes the mark off and deletes
  what no pointer names, as every install does, or adopts a folder of its own
  identity that is whole. The next removal finds no pointer: it says that the
  model is not installed, and before it answers it deletes what no pointer names
  by the installer's own routine (`clean`), each folder when its hold file is free
  by one try, and only after the sync of the model's folder passed; a folder a run
  still holds is left
  (`remove::a_removal_interrupted_after_the_pointer_was_cleared_is_finished_by_the_next_install_or_removal`,
  `remove::a_removal_that_finds_no_pointer_leaves_a_folder_a_run_still_holds_and_deletes_nothing_when_its_sync_is_refused`).
- **The crashes.** A crash is put in place of each event of a removal of an
  installation that nothing holds: from the clearing of the pointer on under six
  ways of losing names, and before it, where only the names of the remover's
  lock, the mark, the count and the status are not durable, under the loss of
  every name and of none. After each, one
  who holds no lock reads the model as installed, whole, with its hold file, or as
  not installed, never as damaged; and the next removal ends with no pointer, no
  mark and nothing under `installations/`
  (`remove::a_crash_at_any_event_of_a_removal_leaves_the_model_installed_or_not_installed`).
- **What no crash shows.** The sync after the deleting: with it taken out the sweep
  passes, since a folder that a crash brings back is deleted by the next to take
  the lock; it is held on the lists of events of the two tests above. And a folder
  half deleted is not produced: the test kit's disk removes a tree as one event
  ("For later tasks").
- **A removal that was refused after it cleared the pointer** answers with the
  disk's refusal, of a model that is not installed and says `removed`; asked
  again, it says not installed, and finishes.

### A folder of the layout that is a link

Code (task M5c, after the review of its pull request: `directory.rs`, `unlinked`;
`Fs::is_link` in the store's file-system interface; tests named `links::…`). The
owner's ruling, 2026-11-15: **refuse, do not follow.**

Five folders of the layout are made by whizbang inside the models directory and
then used wherever they are found: `models/`, a model's folder under it, and the
model's `installations/`, `staging/` and `holds/`. When one of them is a symbolic
link, every name under it is a name of what the link leads to. Until this section
all of them were followed: a removal with `installations/` a link listed the folder
the link led to and deleted every name in it; with `staging/` a link it deleted
that folder's `incoming`; with the model's folder a link it did both there, and
wrote its mark and its status there; with `holds/` a link it deleted nothing
outside, and took its hold files alone out there. The installer's cleaning had
listed and deleted the same way since task M2b; an install made its folders, its
lock file, its staging and its installation through such a link, and set a pointer
there; and a hold shared a hold file through one. Each answered as if nothing were
unusual.

- **A model one of whose five folders is a link is refused** by a removal
  (`RemoveRefused::Linked`), an install and one who attaches (`InstallRefused::Linked`),
  a staging (`StageRefused::Linked`) and a hold (`HoldRefused::Linked`), in one
  sentence of fixed words and no path: "a folder of the models directory is a link,
  and nothing is done through it". Whichever of the five it is, and whatever it
  leads to: a folder, a file, or nothing. For a model the refusal is of all of it:
  a hold is refused for a link at `staging/` as for one at `holds/`. A user who
  finds it takes the link away, or puts the folder back.
- **Nothing is done through it.** The look comes before anything else that touches
  the disk: before a removal's mark, before an install makes a folder or takes the
  model's lock, before a hold takes the directory's lock. So the refused call has
  no event on the shim's list at all, and what is inside the directory and what
  the link leads to are, name for name and byte for byte, as they were; with the
  folder put back the same call goes on
  (`links::a_removal_refuses_a_folder_of_the_layout_that_is_a_link_and_deletes_nothing_through_it`,
  `links::an_install_refuses_a_folder_of_the_layout_that_is_a_link_and_cleans_nothing_through_it`,
  `links::a_hold_is_refused_through_a_folder_of_the_layout_that_is_a_link_and_makes_nothing`:
  each over the five folders, which are the four of the reviewer's probe and
  `models/`).
- **The look is `Fs::is_link`:** the name itself, without following it
  (`symlink_metadata`), each of the five in turn, the model's folder before what is
  in it. It is a call the store's interface was given for this, which reads one
  name and changes nothing; on the test kit's disk it is no event
  (`whizbang-core`'s `fs::a_link_is_seen_at_its_own_name_whatever_it_leads_to_and_nothing_else_is_one`;
  the test kit's `fs_shim::a_link_a_test_made_is_seen_through_the_shim_and_the_look_is_no_event`).
  A file system that does not give the call answers that it is unsupported, and a
  models directory over it is refused, as one over a file system that streams
  nothing is.
- **It is made again before each listing and each deleting,** not once: under the
  model's lock by a removal, which may have waited for it; by a removal again
  when it has waited for the holds, before it clears the pointer, since the look
  under the lock is then a whole wait old, and a removal refused there has
  changed nothing; and once more when the pointer is cleared, just before the
  deleting, where a refusal leaves the model not installed and its folders for
  the next removal
  (`links::a_folder_swapped_for_a_link_while_a_removal_waits_for_a_hold_is_refused_and_changes_nothing`,
  `links::a_folder_swapped_for_a_link_once_the_pointer_is_cleared_is_refused_before_anything_is_deleted`);
  by the installer's
  cleaning before it lists `installations/`, at the start of an install and at its
  end; by a staging before it removes what the last one left; by a publication
  after the download, before it reads back, makes a folder, renames or points;
  and by every unstaging, which removes nothing when it finds a link
  (`links::a_folder_swapped_for_a_link_while_a_removal_waits_for_the_lock_is_refused_once_it_has_it`,
  `links::a_folder_swapped_for_a_link_after_an_install_looked_is_refused_before_it_cleans`,
  `links::a_staging_folder_swapped_for_a_link_after_a_staging_looked_is_not_removed_through`,
  `links::a_folder_swapped_for_a_link_inside_a_download_is_refused_before_anything_is_published`).
- **The directory's own folder may be a link,** and that is the supported way to
  put a models directory on another disk: the whole of it, by a link at the folder
  it is opened at, or by opening it where it is. That folder is not looked at. An
  install, a hold, a second install and a removal through such a link each end as
  they do without it
  (`links::a_models_directory_whose_own_folder_is_a_link_installs_holds_and_removes`).
  One model, or one of its folders, on a disk of its own is not supported by a
  link; a file system mounted at the folder is no link, and is not refused.
- **What is under the five is as it was.** A link under `installations/` is deleted
  itself and never followed ("Publishing"; "Removal": on the disk); the staging
  folder inside `staging/` is removed and made again each time, a link there with
  it ("Downloading into staging"); a pointer, a status, a mark or a record that is
  a link is refused by the read.

**The limit: a look, not a guard.** A folder that is swapped for a link after the
look and before the call that follows it is gone through by that call. The store's
interface names everything by its path, and has no call that opens a folder and
then lists or deletes inside what it opened, so the moment cannot be closed with
what the crate has; closing it needs such calls (`docs/carry-over.md`). The looks
above make the moment short, a few calls of the file system, and do not make it
nothing: with `staging/` swapped inside a download, the pieces that follow are
written where the link leads until the publication looks
(`links::a_folder_swapped_for_a_link_inside_a_download_is_refused_before_anything_is_published`,
which pins that, and that nothing that was there is deleted). The swap needs one
who can write the model's folder, and the directory is its owner's alone (every
folder here is made readable and writable by its owner only): one who can do it
can delete the same files without whizbang. **The moment is accepted** (task N2
of checkpoint 1C, the plan's decision D13): since that task a models directory
that is another user's, or that its group or anyone can write, is refused when it
is opened, so the only one who could make the swap is the directory's owner ("A
models directory is its owner's alone", with its limits;
`directory::a_new_models_directory_is_readable_and_writable_by_its_owner_only`,
`directory::a_models_directory_that_its_group_or_anyone_can_write_is_refused_and_nothing_in_it_is_read`).

**What is not refused.** A reader that holds no lock (`current`, `draining`,
`status`) itself still reads through such a link, and changes nothing: asked
alone, it can answer that a model is installed which every install, hold and
removal then refuses. The inventory, which is built on `current`, looks at the
layout first and answers damaged, so such a model is never called ready ("The
inventory"). And a lock file that is a link ("Limits").

**The owner may later choose to follow links instead,** so that one model can be
kept on another disk by a link at its folder. That would take a rule for what a
removal may delete through one, and is a new decision: this section is what is
refused until then.

### The inventory

Code (task M6: `inventory.rs`, `Installations`; tests named `inventory::…`).
`Installations` is the core's `Inventory` (ADR 0015, "An inventory") over one models
directory: it is made from the directory, a catalogue, each adapter this build has
with the revision it states (no adapter is in the build yet, so whoever makes the
inventory states them: the binary's wiring states none, `wiring::adapters`), and
the executor's secrets. It answers one question, of one
model, from what is on the disk now, each time it is asked. Each test asks it
through an executor the test builds over it (`InstalledRoster::over`), whose roster
offers one local backend.

| The answer | When |
|---|---|
| Not in the catalogue | The catalogue holds no model of exactly that name (`inventory::a_configured_model_that_the_catalogue_does_not_hold_is_prepared_as_not_ready_saying_so`) |
| Does not run on this platform | The catalogue holds the model, and its entry does not name the platform the build is for (`Platform::this`; a build for a platform no catalogue names runs none of its models). Said first, before the adapter and before anything is looked for on the disk, installed or not (task N3b; until then such a model, installed, was ready) (`inventory::a_model_whose_entry_does_not_name_this_platform_is_not_ready_for_that_reason_installed_or_not`). `Installations::on` sets the platform the models are to run on: the binary's wiring calls it with the one its models are given, `Platform::this` unless a test gives another (reworded with task N4, from the review of N3b: it read "gives another platform, for a test", and product code calls it); `unrunnable` is the same question, for whoever lists models with no directory to look in |
| No adapter | The catalogue holds the model, and the build has no adapter of the name its entry gives. Said before anything of the model is looked for on the disk, so it is the answer whether the model is installed, not installed or damaged: nothing this build installs would make it ready (task N3a; until then an installed one was out of date and one that was not was not installed) (`inventory::a_model_whose_adapter_this_build_lacks_is_not_ready_for_that_reason_installed_or_not`) |
| Not installed | The model has no pointer: never installed, or removed |
| Ready, under the installation's identity | The pointer names an installation whose record is the record an install would make now: from the catalogue's entry, under the revision this build's adapter states (`inventory::a_model_in_the_catalogue_is_ready_when_installed_and_not_installed_otherwise`) |
| Out of date | The pointer names an installation with any other record: another revision of the entry, or another revision of the adapter. Two revisions are the same or they are not; nothing is newer (`inventory::an_installation_whose_recorded_catalogue_or_adapter_revision_is_not_the_current_one_is_prepared_as_out_of_date`) |
| Damaged | What `current` reports as damage ("The pointer"): a pointer that is not one as it is written, a record that is not there, is not in the one form, is another installation's or holds a secret the inventory was told of; a disk that cannot be read, by a call it refuses or a permission it does not give (`inventory::a_pointer_or_a_record_that_cannot_be_read_is_prepared_as_damaged_never_ready_and_never_not_installed`, `inventory::a_pointer_or_a_record_the_system_refuses_permission_to_read_is_damaged_and_never_not_installed`); and a model one of whose layout folders is a link, which every install, hold and removal refuses (`inventory::a_model_one_of_whose_layout_folders_is_a_link_is_damaged_and_never_ready`). Never ready, and never "not installed"; a new install repairs it, but for the link, which its owner takes away (`inventory::an_installation_record_that_is_damaged_or_a_pointer_that_names_nothing_is_prepared_as_damaged_and_a_new_install_repairs_it`, `inventory::a_record_that_holds_a_secret_the_inventory_was_told_of_is_damage_and_what_is_said_holds_nothing_of_the_disk`) |

- **The name it is asked by is never a path.** It is the name a backend declares
  for its model, which comes from a config and may hold a separator, dots, anything.
  It is compared with the names of the catalogue's entries, exactly, with no case
  folded and nothing cut; a name the catalogue does not hold opens nothing. The
  folder that is read is named from the catalogue's own entry, whose name is a
  `ModelName`; and an entry whose name holds a secret the inventory was told of,
  in a catalogue made by a reader that was told of others, is not looked for on
  the disk
  (`inventory::a_declared_name_is_looked_up_in_the_catalogue_and_never_used_as_a_path`).
- **It holds no lock, and reads the pointer and the record it names,** as "The
  pointer" says of a reader with no lock. It makes nothing: asked of a model that
  was never installed, no folder is made.
- **Rascal's R2 and R4.** An installation of an entry or an adapter that has since
  changed is reported, as out of date; and a configured model that the catalogue
  does not hold is never ready: its call is prepared as not ready and refused at
  admission, with the reason.
- **After a reinstall** under another revision, the installation is another
  identity, which is the model's version in what a preparation saw: a request that
  carries the old preparation is refused as `Stale::Model`; one under a new id with
  no preparation is admitted and run under the new installation's identity; and a
  repeat of the first request's id returns the first run, and runs nothing
  (`inventory::after_a_reinstall_the_old_preparation_is_stale_a_new_request_runs_on_the_new_installation_and_a_repeat_returns_the_first_run`).
  A repair, which has the identity of what it repairs, leaves a preparation good.
- **Limits.** *It does not read an installation's files:* "verified" is what the
  publication held them to, and a file damaged since is found when the identity
  is installed again
  (`inventory::known_an_installation_whose_file_was_damaged_after_it_was_published_is_ready_until_it_is_installed_again`).
  A hold does not read them either ("Holds"), so nothing stands between a file
  damaged after its publication and the first adapter that loads it: that
  adapter's task verifies, at the load or at the first lease in a process (1C,
  `docs/carry-over.md`).
  *In the inventory's own tests no request reaches a model:* "runs under the new
  installation" is there what the run's action is handed as its model's version.
  A run is handed its backend under that installation, and each request is
  leased ("The lease"). *The draining
  mark is not read:* a model whose removal waits for holds is ready, and it is the
  lease that is refused then (task M6b); one whose pointer a removal has cleared
  is not installed. *An adapter this build does not have* has a reason of its
  own since task N3a (the table above), and `whizbang models list` says it
  (`models::a_model_whose_adapter_this_build_lacks_is_listed_with_that_reason`). *What a
  config offers is put over an inventory* since task W1 (ADR 0015, "What a config
  offers, over an inventory"), and the binary's wiring makes this one
  (`wiring::Over::installed`, "Where the directory is"); the tests here give a
  roster.

### The lease

Code (task M6b: `inventory.rs`, `Lease` and `Installations::hold`; tests named
`leases::…`). **The lease is what stands in the lease's place** of an executor over
the inventory (ADR 0015, "A wrapper around each request", `Place::Lease`;
`Engine::with_wrapper`): `Lease::over` an `Installations`. A run admitted for a local
model is handed that model's backend under the identity of the installation the
admission recorded (ADR 0015, "The backend a run is handed, and the lease's place"),
and the lease is told that identity with every request of the run. Each test asks
through an executor the test builds over the real inventory, with a fake meter in
the place after the lease and a fake segmenter behind it.

- **Before a request to a local model leaves, the lease takes a hold** on that
  installation of that model, `Installations::hold(model, admitted)`: a hold of
  "Holds" (`Holds::hold`), by the installation's identity, asked by the name the
  backend declares. With it the request goes on to the meter's place and the
  backend, and **the hold is let go when the request is over,** so the installation
  is held exactly while a request is with its backend: taken alone by nobody
  then, and free again after
  (`leases::a_reinstall_or_a_removal_between_the_admission_and_the_lease_refuses_the_request_before_dispatch`,
  its last part).
- **The name is looked up in the catalogue,** as the inventory looks it up, and is
  never a path; a name the catalogue does not hold names no installation
  (`leases::a_hold_is_asked_by_a_declared_name_that_is_looked_up_in_the_catalogue`).
  The inventory keeps the holds of its process (`Holds`), so two requests of one
  process on one installation are one share of the hold file and a count. The
  same test asks, with the model installed and ready, for a hold on another
  identity: refused as not named, and the installation that is current is not
  held in its place.
- **It is asked for the identity the admission recorded, not the one current at the
  request,** and a lease that cannot be had refuses the request before dispatch.
  When the pointer no longer names that installation, after a reinstall under
  another revision or a removal, the hold is refused (`NotNamed`); while the model
  is draining it is refused (`Draining`). The request then reaches neither the
  meter's place nor the backend: its action is answered a fault that says nothing
  left (`rejected`, not sent), in fixed words that hold no name, no path and no
  identity, and the refusal is noted in the run's record as a request that was not
  ready. **No other installation answers in its place:** the hold is by identity,
  and there is no call that takes "whatever is current". With a barrier between
  the admission and the lease, a reinstall there, a removal there and a removal
  that is draining there each end so, the fake meter and the backend having seen
  nothing; with nothing in between the request is answered. What a run answers
  when its lease is refused as draining is in that test too: its action is told
  that nothing left, the run ends as its action ends it, and its record holds
  the refusal
  (`leases::a_reinstall_or_a_removal_between_the_admission_and_the_lease_refuses_the_request_before_dispatch`).
- **Never whatever is ready.** With the lease over an inventory that calls the
  reinstalled model ready, under its new installation, a run admitted with the
  old one is refused all the same, and the new installation is not held
  (`leases::a_lease_is_for_the_installation_that_was_admitted_though_the_reinstalled_model_is_ready`).
  A backend must load by that identity too, and not by the pointer: that is the
  first real local backend's (1C, `docs/carry-over.md`).
- **A retry inside an admitted run is leased again, for the same identity.** A run
  whose installation was replaced after its first request is refused at its second
  the same way, and the first's hold kept the superseded installation whole while
  it was asked
  (`leases::a_retry_inside_an_admitted_run_whose_installation_was_replaced_is_refused_the_same_way`).
- **A removal that starts while a request holds the model waits for it,** and goes
  on when the request is over
  (`leases::a_removal_that_starts_while_a_request_holds_the_model_waits_for_it`).
- **The hold goes when what the lease holds is dropped,** and not only when the
  lease is told how the request ended: when the backend panics under it, and when
  the place after it panics before the request leaves, the installation was held
  at that moment and is free afterwards
  (`leases::a_lease_lets_its_hold_go_when_the_backend_panics_and_when_the_place_after_it_panics`).
  The drop does nothing that can panic: it counts the process's holds on the
  installation down under a guard that takes a poisoned lock as it is, and lets
  the share of the hold file go with the last (`Hold::let_go`).
- **What the lease does not hold.** A backend that names a recipient answers with
  no model of this machine: its requests go by, and nothing is held for them. A
  local backend that comes under a version that is no installation's identity
  was admitted over no inventory: the lease has nothing to hold it to, and
  refuses it
  (`leases::a_backend_that_names_a_recipient_takes_no_lease_and_a_local_one_with_no_installation_is_refused`).
- **Limits.** *A hold does not read the installation's files* ("Holds"; the
  inventory's limit on unverified files, `docs/carry-over.md`). *A call with two
  needs of one capability* is leased for the first's installation only (ADR 0015,
  "An inventory", limits). *The lease is in the binary's executor* since its wiring
  (the plan's task W1; ADR 0015, "The binary wired"), wherever the wiring is given
  a models directory; no command names one yet (1C, `docs/carry-over.md`).
- **The checkpoint's exit line for models** ("a model installs, attaches, cancels
  and drains with fake downloads", `SPEC.md` section 16) is one test, with this
  process on one side and a real second process on the other, each through this
  crate's interface: an install here that the other attaches to and that is
  cancelled; an install here that the other attaches to and that publishes; and a
  removal here that drains the other's hold, busy at its deadline and done once
  the hold is let go
  (`leases::a_model_installs_is_attached_to_is_cancelled_and_is_drained_from_this_process_and_a_second_one`).

### What a refusal says

Code (`refused.rs`). A `Refused` is fixed words, a place in a record and numbers
(`refusals::every_refusal_says_one_sentence_of_fixed_words`). **It quotes nothing of
what was refused,** secret or not, as ADR 0012's refusals do: a refusal that quotes
nothing cannot show what the reader was not told is a secret, and needs no escape. A
value in a record is named by its place, as field names and positions
(`models[0].files[1].size`); an unknown field is named by the object that holds it. **A
place is built only from the field names of the format and from positions,** never
from a key the file gave. It holds no path: the readers take bytes, or a source, and never learn one.

### Limits, each pinned or stated

- **A secret is found only when the reader was told of it.** A key written into a
  catalogue's address that the executor never heard of is kept in the address.
- **A revision is its author's word.** Nothing here can tell that an entry changed and
  its revision did not. Nothing orders two revisions: they are the same or they are not.
- **A field given twice hides what it held.** Such bytes are refused for the second
  field, and what either held is not looked at for a secret as decoded (step 5 comes
  after step 4). They are refused either way, and a refusal shows nothing.
- **The line of a refusal is the reader's,** and can be late. A field given twice is
  refused at the line of what follows its second key: with the key on one line and its
  colon or its value on a later one, that later line. And a line break written raw
  inside a text, which JSON does not allow, is refused at the line after it.
- **A megabyte of JSON may take some tens of megabytes while it is read:** a list of
  many small values in a field nobody knows is read before it is refused
  (`refusals::values_nested_past_what_the_reader_follows_are_refused_and_not_followed`).
- **Names are ASCII.** A file whose maker named it with other letters is renamed in its
  catalogue entry.
- **A link where a lock file belongs is followed:** a model's `LOCK` that is a
  link locks, or makes, the file it leads to, and so does the directory's, and a
  hold file that is a link is locked where it leads (`docs/carry-over.md`, the row
  on a file read piece by piece). Whoever can put a link there can write the
  directory itself, and since task N2 of checkpoint 1C that is its owner alone ("A
  models directory is its owner's alone"): accepted so, with a pipe put under the
  name of a file that is read. **A link where one of the fixed folders belongs is refused,**
  since the review of task M5c's pull request: `models/`, a model's folder,
  `staging/`, `installations/` and `holds/` ("A folder of the layout that is a
  link", with its own limit: a folder swapped for a link between the look and the
  call after it). Until then each was used as it was found, link or folder, and
  this record said so here. The staging folder inside `staging/` is removed and
  made again each time, so a link there is not followed. **A link where an installation's folder belongs** is followed when a
  publication of that identity reads what is there, and is removed, itself, when the
  pointer does not name it.
- **The last sync of a publication.** When the sync that makes the new pointer
  durable is refused, the install is refused and the pointer names the new
  installation, which is whole: the rename has happened, and is not taken back.
  Until a sync of the model's folder passes, the installation the pointer named
  before is not deleted, so a crash then leaves the old pointer with its
  installation whole, or the new one with its own. Asking again syncs before it
  answers
  (`publishing::each_step_of_the_file_system_refused_leaves_the_previous_installation_or_the_new_one_whole`,
  `publishing::the_pointers_sync_refused_and_then_a_crash_leaves_the_old_installation_or_the_new_one_whole`).
- **The directory's lock is tried, not waited for,** until five seconds of the
  clock have passed, and 20,000 times at most ("Three kinds of lock"). Under a
  holder that has not let it go by then, a publication is
  refused as busy with its installation renamed in and not named; the folder is
  left, and the next install of that identity points to it without a download. A
  repair refused so after it cleared the pointer leaves the model not installed.
  When the installation was held and the pointer could not be put back, the model is
  not installed while a run still holds the damaged folder: no pointer names it, no
  new hold is given on it, and the next install of that identity is refused as held
  until the run lets it go
  (`publishing::a_held_repair_that_could_not_put_the_pointer_back_leaves_the_model_not_installed_and_the_next_is_refused_as_held`).
  A lock taken alone that waits is not in the store's interface, and one that
  waits with no end is not wanted: the tries stay (`docs/carry-over.md`, the line
  that accepts them). And one who asks for several things under the lock in turn
  asks for it each time: each asking has its own five seconds. **A removal asks
  at least three times:** to set its mark (and raise its count of removals), to
  clear the pointer, and to take its mark off; once more when what it finds at
  the mark's name, once it has the model's lock, is not the mark as whizbang
  writes it; and once more between two tries, while it waits for the model's lock
  or for a hold, when its mark is gone. Its tries for the remover's lock ask for
  it not at all. Its deadline is looked at between two askings and never inside
  one. So a removal that waits for nothing can take some fifteen seconds under a
  directory whose lock is given each time just inside the five, twenty when its
  mark was replaced; and one that waits can overrun its patience by the five
  seconds of the asking it was in. An asking that is not given by then ends the
  removal as busy at once. The sum is rough: it is counted from the code as task
  P5a of 1C left it.
- **An install that is refused every write of its status.** "While an installer has
  the model's lock the status says `installing`" rests on the install's first step,
  the write of that status. When it is refused, the install is refused and has
  touched nothing else; it writes how it ended, and when that write is refused too
  (a model's folder that cannot be written), the status is still what the install
  before it left. So a repair of a damaged installation in use, refused so, leaves
  the old `installed` under a lock an installer holds: one who attached meanwhile is
  shown nothing, and is then told that the model is installed, of files nobody
  read. The pointer does name that installation; that its files are whole was held
  when they were published, and by nothing since (the limit on a reader that holds
  no lock, below)
  (`attaching::known_an_install_refused_every_write_of_its_status_leaves_the_last_status_and_one_who_attached_is_answered_from_it`).
- **A request that met only another's share of the model's lock.** One who attached
  has a share of the model's lock for the moment in which it reads the status and
  the pointer. An install tried in that moment is busy, so the request that asked
  for it attaches, to no install: it is given a share at once, `watch` is not
  called, and it is answered by how the last install ended. `installed`, with the
  pointer naming the installation it asked for, is true, and nothing needed to run.
  `failed` or `cancelled` is how an earlier request's install ended, not this one's:
  asked again, the request installs
  (`attaching::known_a_request_that_met_only_anothers_share_of_the_lock_is_answered_from_the_last_installs_status`).
- **A mark that is left.** A removal takes its mark off at its end, and the step
  can be refused: the removal answers as it ended, and the mark stays. So does the
  mark of a remover that died. Neither stops anybody once its remover's lock is
  let go (task P5a of 1C: "Removal", "A mark is live only while its remover is";
  until then no hold on the model was given and no install of it published while
  the mark was there). The next removal of the model takes it off, and so does
  the next install (task M5d).
- **A reader that holds no lock does not read the files.** `current` answers from the
  pointer and the record; that the files are whole is held when they are published
  and when the same identity is published again.
- **A name that is one plain name on macOS and Linux may be none elsewhere.** A name
  that ends in a dot, and the names Windows keeps for devices (`con`, `nul`, `aux` and
  the like), are accepted: the platforms are macOS and Linux (`SPEC.md` section 1).
- **Not in `catalogue/2`:** a credential for a download that needs one. It comes
  with the task that reads one (plan 1C, task N4), and is on `docs/carry-over.md`.
  A model's title and the platforms it runs on came with `catalogue/2` (task N3a).
- **A platform is asked by nobody yet.** An entry says which platforms its model
  runs on, and the inventory does not ask: on a platform the entry does not name,
  a model that is installed is ready. The command that lists and gets models says
  so and refuses to get one (plan 1C, task N3b).

### For later tasks

- **M2** is handed a `Downloader` (ADR 0012) and an `Entry`. Each request is for a
  file's `from` or for an address a download was moved to, and the second only when
  `Entry::may_download_from` answers yes for its origin. One byte more than a file's
  `size` is the most it asks for, and its `size` is the most its sink keeps
  ("Downloading into staging").
- **M2** names nothing in staging by adding a suffix or a prefix to a `FileName` in a
  folder that also holds plain file names: `a` with `.part` added is the plain name
  `a.part`. A file being downloaded has a folder of its own kind, or a name that no
  `FileName` can be.
- **M2b** names an installation's folder by the hex of its identity, writes `RECORD` as
  `InstallationRecord::to_bytes` and reads it back with `InstallationRecord::parse`. A
  record whose identity is not the name of its folder, or not what the pointer says, is
  damaged.
- **M2b: the folder is already there.** The rename of step 1 fails when
  `installations/<id>` exists: the same identity, superseded earlier and still held, or
  left by a crash. The installer verifies the folder that is there against the record,
  and with it whole points to it and drops its staging; with it damaged and its hold
  file free it deletes it and renames; with it damaged and held it is refused as held. Under
  the directory's lock a publication does the reading of the mark, the making of the
  hold file and the replacing of the pointer, with their two syncs, and nothing else:
  no verifying and no deleting.
- **M2b: the pointer is read back as it is written.** Its content is the identity's
  text and one line feed: the reader checks that there are 72 bytes and that the last
  is a line feed, takes that one byte off, reads the rest with
  `InstallationId::from_text`, and refuses everything else. No other white space is
  taken off.
- **M2b and M6: what is refused is damage.** A pointer or a record that
  `InstallationId::from_text` or `InstallationRecord::parse` refuses, for a secret
  (`Refused::HoldsSecret`) as for anything else, is reported as damage, never as "not
  installed". A record whose `identity().hex()` is not the name of its folder is
  damaged: that comparison is the installer's and the inventory's, and no reader of a
  record makes it.
- **M2b reads a staged file back before it trusts it,** through the store's streamed
  read (ADR 0018, `open_file`): it counts the bytes and stops one past the size, and
  holds the file to its length and its hash, both, never to its length alone. A
  crash can leave a file of the right length with bytes that were never written, so
  a test damages a staged file by hand, keeping its length, and the verifier refuses
  it by its hash; no crash test shows that. A link at the file's own name is refused
  by the read; a hard link is read as the file.
- **M2b's crash sweeps run under every choice of what a crash leaves of unsynced
  bytes:** half, the full length zeroed, all, none. Nothing a crash left of a file is
  ever a place to go on from: what is found in staging is removed.
- **M2b: the adapter's revision comes from the adapter.** `InstallationRecord::of`
  takes it from its caller, who gets it from the adapter the entry names, and from
  nowhere else.
- **M6 compares two revisions by equality only.** Nothing is newer or older. Built
  ("The inventory").
- **M5b and M5d:** the status record says whether the holder of the model's lock is an
  installer or a remover, so that one who attaches knows what it is waiting for. An
  installer says `installing` from its first step (task M3), and a remover
  `removing` (task M5b, `status/2`). Built ("The status record").
- **M5b:** the draining mark is read by a publication as there or not there
  (`DRAINING` in the model's folder), under the directory's lock. A mark with content
  is read the same way. Built ("Removal"). A stale mark stops every publication of
  the model, and every hold on it, until a removal of the model or the task that
  has the next to take the lock take one off (M5d). *Changed by task P5a of 1C:*
  a publication asks `draining`, and a stale mark stops nothing.
- **M5d, from task M5b:** an install that takes a stale mark off takes a live one
  off with it, so a removal sets its mark again; an install reads the mark where
  it asks its cancellation. Built ("Removal"). *Changed by task P5a of 1C:* an
  install takes off only the mark of a remover that died.
- **M5c, from task M5b:** the hold files a removal has alone when it clears the
  pointer are the ones it deletes under; it lets them go only after the deleting
  and its sync. A removal that finds no pointer still deletes what no pointer
  names, as `clean` does, after the same sync of the model's folder. Built
  ("Removal": on the disk).
- **M5: the store's try for a share** (ADR 0018, `try_share`) is refused by the
  process's own lock on the file alone, and given beside its own shares as a second
  lock. A file system that answers that it cannot try is an error, never "held
  alone". Built ("Holds").
- **M5:** the count inside a process does not stand in for the check: every hold reads
  the pointer and the mark under the directory's lock, whatever the count. Letting the
  last hold go and taking a new one in another thread go through one guard inside the
  process, so that the share is not let go under a hold just counted. A hold on an
  installation whose hold file is missing is refused as damage; a hold never makes one.
  Built ("Holds").
- **M5b, from task M5:** a hold is refused as draining by a file at `DRAINING`,
  whatever it holds, read with the pointer under the directory's lock (since task
  P5a of 1C, only while its remover is alive). A removal
  that answers busy has changed no pointer (task M5b), so a hold that is given
  after it finds the pointer as it was. And a removal that waits for holds meets
  its own process's: `Holds` lets its share go only with the last hold.
- **M5b and M6:** a reader that holds no lock reads the mark as well as the pointer
  (`draining`, beside `current`), and can say "draining", not "not installed", of a
  model whose pointer a removal has cleared and whose mark is still set. The
  inventory does not: the core's `Unready` has no such reason, and a removal clears
  the pointer and takes its mark off within a few calls of the file system. A
  command that lists models is where it is said (1C).
- **M5c:** a folder under `installations/` that the pointer does not name and that has
  no hold file was never published: it is deleted, and no hold file is made for it.
- **The crashes the tests of M2b and M5c hold,** each reopened by the next to take the
  model's lock. The first three rows and the last are held by task M2b, as below
  the table; the two of removal are task M5c's ("Removal": on the disk). "Step 1" in it is the rename of the
  staged folder and its sync (steps 5 and 6 of "Publishing"), and "step 2" the pointer
  (step 7). Where a row says the next deletes a folder the pointer does not name, the
  next is a removal or an install of another identity: an install of the same
  identity points to the folder when it is whole, and deletes it when it is not:

  | A crash | What is on the disk | What the next does | The model is |
  |---|---|---|---|
  | Publishing, after step 1 | A folder the pointer does not name, with no hold file, or with one from an earlier publication of that identity | Deletes it, taking the hold file alone when there is one | As it was |
  | Publishing, inside step 2, before the pointer's rename is durable | The same, with the hold file made | Takes the hold file alone and deletes the folder | As it was |
  | Publishing, after the pointer's rename is durable | The new pointer; the old folder, superseded | Cleans the old folder when its hold file is free | The new installation |
  | Removal, after step 1 and until its pointer's removal is durable (step 5) | The mark; the pointer as it was | Takes the mark off | Installed |
  | Removal, after its pointer's removal is durable | The mark; no pointer; the folder | Finishes the removal: deletes the folder, takes the mark off | Not installed |
  | Cleaning a superseded folder, or a repair, part-way through deleting | Part of a folder the pointer does not name | Deletes the rest | As the pointer says: for a repair, not installed |

  **How task M2b holds them.** A crash is put in place of each event of an install
  in turn (ADR 0018, `Lost` and `Tail`). At each event the names that were not
  durable are lost every way of a fixed six (all, none, those of every other folder,
  every other name), and then one by one, by what that crash had pending: each name
  alone lost and each alone kept, and each folder that held such names. From the
  first staged file begun to the event after the last is finished, all of that is
  tried under each of the four ways of leaving what was appended to a file and not
  synced; at every other event under one of the four, another from event to event,
  since nothing appended is unsynced there: each staged file is synced before it is
  finished, which the sweep holds of its own list of events. After each crash, one
  who holds no lock reads the model as it was, or as the new installation, never as
  damaged; what the pointer names is whole and has its hold file; and the next to
  take the model's lock ends with one installation, whole, no staging and no
  pointer half-written. Three installs are swept, each cut into parts so that a test
  stays short:
  - into an empty directory, reopened by an install of the same identity, which
    finds what the crash left under `installations/` and points to it or replaces
    it
    (`crashes::a_crash_at_any_event_of_a_first_install_leaves_it_not_installed_or_whole_part_1_of_4`
    to `…part_4_of_4`);
  - over another installation, reopened by an install of a third identity, which
    deletes what the crash left and what was superseded: the first three rows
    (`crashes::a_crash_at_any_event_of_an_install_leaves_the_previous_or_the_new_whole_part_1_of_4`
    to `…part_4_of_4`);
  - a repair of a damaged installation in use, where the model is also read as not
    installed once the pointer was cleared: the last row
    (`crashes::a_crash_at_any_event_of_a_repair_leaves_it_as_it_was_not_installed_or_whole_part_1_of_4`
    to `…part_4_of_4`).
  - **a refused step and then a crash:** each event of each of the three installs
    refused in turn, and the crash straight after, under the same losses and tails.
    Under the loss of every name and of none, the install is also asked again
    before the crash: its answer is given only for what the crash then leaves
    (`crashes::a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_1_of_8`
    to `…part_8_of_8`, in eight parts since the exit check of checkpoint 1B, where
    its first two of four had come near the limit on one test's time,
    `crashes::a_crash_at_any_event_after_a_refused_step_of_an_install_leaves_the_previous_or_the_new_whole_part_1_of_4`
    to `…part_4_of_4`,
    `crashes::a_crash_at_any_event_after_a_refused_step_of_a_repair_leaves_it_as_it_was_not_installed_or_whole_part_1_of_4`
    to `…part_4_of_4`).
    A publication refused after its folder was renamed in leaves that folder whole,
    named by no pointer, with a hold file or with none ("Publishing"): after a
    refusal that is a state like any other, and the crash and the next install
    meet it. Each step from the rename of the staged folder to the pointer's
    rename is refused, and the crash follows: the pointer names what it named; the
    next install of that identity points to the folder and, when the crash lost no
    name, asks for no byte; the next of another identity deletes it
    (`crashes::a_crash_at_any_event_after_a_refused_step_of_a_publication_leaves_its_folder_for_the_next`).

  **What a crash does not show,** and what stands in for it. A name that is not
  durable follows its folder through a rename on the test kit's disk (ADR 0018), so
  an installation renamed into place without its `RECORD` or without a file is a
  state the sweeps reach: with the sync of the staging folder after `RECORD` is
  written taken out, or the sync of `files/`, a reader finds damage and the sweeps
  fail. Four syncs are not shown by a crash of an install: with any one of them
  taken out the sweeps pass, and another test holds it. The one that follows the deleting of a folder: a deleted folder that a crash
  brings back is deleted again by the next to take the lock, and in a repair the sync
  after the rename covers the same folder; it is held on the list of events
  (`publishing::a_repair_clears_the_pointer_deletes_the_damaged_installation_and_then_publishes`,
  `publishing::a_superseded_installation_is_deleted_when_no_hold_is_on_it_and_left_while_one_is`).
  The two of staging before any file is asked for, of `staging/` once the staging
  folder is made and of the staging folder once `files/` is made: the staging folder
  is synced again after `RECORD` is written, and what a crash leaves in staging is
  removed, whatever it is (`staging::two_files_are_staged_and_verified`). And the one
  after a refused staging is removed: what a crash brings back of it is removed by the
  next to take the lock
  (`staging::a_wrong_hash_fails_the_download_and_leaves_no_staged_file_that_passed`,
  `staging::a_short_file_fails_the_download_and_leaves_no_staged_file_that_passed`,
  `staging::a_file_one_byte_too_long_fails_the_download_and_leaves_no_staged_file_that_passed`).
  **A folder half deleted is not produced either:** the test kit's disk removes a
  tree as one event, so the last row of the table is reached only as a folder that
  is not whole because a crash lost names in it after it was renamed in, never as
  one a deletion was part-way through.
  A reader does not read an installation's files, so "whole" after a crash is the
  test's own reading of every byte, not the reader's word.
- **A process that dies inside a download** (rascal's untested case, a signal during
  a download): stopped at a barrier inside its second file, it has the model's lock,
  and another process is busy and touches nothing; killed there, under each tail, it
  leaves the previous installation named and whole, and the next to take the lock
  removes its staging under the lock and asks for every byte again
  (`crashes::a_process_that_dies_inside_a_download_leaves_the_previous_installation_and_the_next_installs`).
- **Two installers over one directory, each a process of its own.** The pointer is
  read under the model's lock, so the second never has a pointer to decide from: with
  the first stopped after its read, the second is busy; let go together, they end in
  one of the two orders, or one is busy, and one installation is left, whole. A
  thread on a disk of its own stands for a process, since the locks are on the open
  file and not on the process ("Three kinds of lock")
  (`crashes::two_installers_stopped_after_one_read_the_pointer_do_not_both_publish`,
  `crashes::two_installers_let_go_together_end_in_one_of_the_two_orders_with_one_installation_whole`);
  and the first of the two cases is held again with the stopped installer in a real
  second process, the test's own program run once more, which says on its output
  that it has stopped and goes on when it is told to
  (`crashes::two_installers_in_two_processes_stopped_after_one_read_the_pointer_do_not_both_publish`).
  "Stopped after each read the pointer" cannot be had: only the holder of the
  model's lock reads the pointer to decide from, and that is what the tests show.
- **M2 and M2b** make every name on a disk from a `ModelName`, a `FileName` or an
  `InstallationId`, and each plants a secret and searches every name and byte it wrote.
  A catalogue made by a constructor that was told of other secrets than the installer's
  is checked again by the installer.
- **M6** compares `catalogue_revision` with the entry's revision, and
  `adapter_revision` with the one the adapter states. Built: it compares the whole
  record with the one an install would make now ("The inventory").
- **1C's real downloader** makes the `Address` a redirect points to itself, and is told
  the executor's secrets to make it (ADR 0012, "For later tasks").

## Consequences

- A changed adapter is a new installation with a new identity, which rascal's was not.
  A call prepared against the old one is stale.
- The installer, the holds, removal and the inventory are built on types that cannot
  hold a name with a separator in it, a size of zero, or a hash of the wrong length.
- The catalogue is JSON, not the TOML of the config. `whizbang-backends` gains `serde`
  and `serde_json`, which the workspace already holds, and no new crate.
- A file two models share is installed once for each, until the task that shares it
  (1C; `docs/carry-over.md`).
- Reversing the identity costs every installation on a disk: each is installed again.
  Reversing the layout costs the same. A change to either record is a new format,
  `catalogue/2` or `installation/2`, by a new decision record.

## Alternatives

- **Hash the record's JSON for the identity,** as rascal did. Turned down by ADR 0001.
- **Leave the adapter's revision out of the identity,** and compare it at readiness
  alone. Turned down: an installation's identity is the version a preparation saw, and
  a changed adapter must make a preparation stale, not only a status line.
- **The hold file inside the installation's folder.** Turned down: removal deletes that
  folder and a reinstall of the same identity makes it again, so a lock file would be
  removed and replaced, which no lock file may be; and the folder would hold more than
  its record and its files.
- **A mark on a superseded installation.** Turned down: the pointer already says which
  installation is in use, and a second statement of it could disagree.
- **Delete the superseded installation inside the install that replaced it, waiting for
  its holds.** Turned down: an install would wait on a run.
- **The catalogue in TOML,** as rascal's was and the config is. Turned down: the
  workspace's TOML reader is the core's alone, and the same strict reader serves the
  catalogue and the record.
- **Read with derived readers.** Turned down: a derived reader takes an object's values
  as a list in the order of its fields, takes a field given twice or reports it in
  words that quote the field, and says of a missing field what the library says. The
  walk by hand makes every refusal here.
- **Quote the refused name, escaped and cut.** Turned down, as in ADR 0012.
- **One address a file, and no list of origins.** Turned down: a file is often served
  from another origin than the one first asked, and the place to say which origins a
  download may reach is beside the files.
- **A model's folder directly in the models directory,** as rascal had it. Turned down:
  a model named `lock` would be the directory's `LOCK` on a disk that does not tell the
  cases apart.
- **A hold as a file that names a process,** as rascal had it. Turned down by the plan:
  a process id is reused, and a file outlives its process.
- **A hold that waits.** Turned down: a wait for a hold file under the directory's lock
  would stop every model, and a wait outside it decides on a pointer read before the
  wait.
- **One lock for everything.** Turned down: a download takes minutes, and a hold on
  another model, or on this one's installation in use, must not wait for it.
