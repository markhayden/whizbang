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
Spec: section 3, "Storage"; section 4, "Segmenters"; section 6, "Controls from table-manners"

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
`install.rs` and `status.rs`. Its tests are in
`crates/whizbang-backends/tests/it/models/`, named below as `names::…`, `catalogue::…`,
`installation::…`, `identity::…`, `refusals::…`, `downloads::…`, `staging::…`,
`publishing::…` and `status::…`.

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
| Publishing; the pointer; the status record; the deleting of what the pointer does not name | Code, with the tests named (task M2b). The crashes of "For later tasks" are held by the task's second pull request |
| Of the layout and the locks: what an install makes and takes | Code (tasks M2, M2b); each part says which |
| Attaching to an install; holds; removal and its mark | Decided, not built: tasks M3, M5 to M5c, M6 |

### The directory

*Decided; of it, task M2 built `models/<model>/`, the model's `LOCK` taken alone by a
try, and `staging/incoming/files/`; and task M2b built the directory's `LOCK`,
`CURRENT`, `STATUS`, `holds/` with the hold file a publication makes, and
`installations/`. `DRAINING` is read as there or not there, and nothing writes it.
The rest is not built.* A models directory is one folder, given by whoever opens it: no
default place and no variable is read for it here. In it:

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
- **The content of `DRAINING`** is decided by the task that builds it (M5b). The
  content of `STATUS` is under "The status record", and the one staging folder under
  "Downloading into staging".

### Three kinds of lock, and their order

*Decided; of it, task M2 built the model's lock taken alone by a try, and task M2b
the directory's lock around a publication, the making of a hold file, and a hold file
taken alone by whoever deletes an installation's folder. The rest is not built: a
model's lock shared, a hold's share, and the mark.* All three are the locks of the store's file-system interface
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
  **How it is taken.** The store's interface has no lock taken alone that waits, so
  the directory's lock is tried, 20,000 times at most, giving way to other threads
  between tries and sleeping nowhere; whoever has it lets it go after a few calls of
  the file system. One who is not given it by then is refused as busy; a
  publication has changed no pointer, and a repair may have cleared one
  (`publishing::a_directory_whose_lock_is_not_let_go_refuses_the_publication_and_names_what_it_named`).
- **A model's lock** (`models/<model>/LOCK`) is taken alone by one install of that
  model, for the whole of it, a download included; and by a removal for its work on
  the disk, after its mark is set ("Removal", below). So one install of a model runs
  at a time, and whoever asks for a model that is being installed attaches by sharing
  this lock, which waits. Staging is written, and cleaned, only by its holder.
- **A hold file** (`models/<model>/holds/<64 hex digits>`) is shared by each hold on
  that installation, and taken alone by whoever deletes the installation's folder.

**The order.** What a process may wait for depends on what it holds:

| Holding | May wait for | Never |
|---|---|---|
| Nothing | The directory's lock; a model's lock, to share it | A lock taken alone is tried and never waited for: the model's lock by an install or a removal, a hold file by whoever deletes |
| The directory's lock | Nothing | Takes no model's lock. Takes a hold file only to share it, as below, which does not wait. Waits for no hold |
| A model's lock, alone | The directory's lock | Waits for no hold file: it takes one alone by a try that does not wait, or not at all |
| A model's lock, shared (attached) | Nothing but the lock it shares | Takes no other lock |
| A hold file, alone | Nothing | Takes no lock and waits for none: it deletes the folder, syncs, and lets the hold file go before it takes the directory's lock or sets a pointer |
| A hold | The directory's lock, to take another hold | Never waits for that model's lock, alone or shared: a run that holds a model neither installs it, removes it nor attaches to its install |

- **A hold is taken only under the directory's lock,** after the pointer was read and
  names that installation and no draining mark is set. Its share of the hold file is a
  try that does not wait, and is not refused there: a hold file is held alone only while its installation's folder is
  being deleted, by a holder of the model's lock, after the pointer stopped naming
  that installation; and the pointer cannot name it again until that holder has let
  the hold file go, which it does as soon as the folder is deleted, before it takes the
  directory's lock again and before any pointer is set.
- **A hold file is taken alone only by a try that does not wait,** by a holder of the
  model's lock, and only for an installation the pointer does not name.
- So the directory's lock is last in every chain, and its holder waits for nothing;
  nobody waits for a hold; and a holder of a hold never waits for the model's lock.
  There is no cycle.

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
  and every byte they wrote for a planted secret.
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
| `revision` | The revision of this entry, a `Revision`. Whoever changes anything of an entry changes its revision |
| `adapter` | The adapter that runs the model, by its name, a `ModelName` |
| `licence` | The licence the files are under, as their maker states it |
| `features` | The features the adapter gives with this model: at most 16 (`FEATURES_LIMIT`), each a feature's name as a backend declares one (ADR 0006), each stated once |
| `origins` | The origins a download of the files may reach: at most 8 (`ORIGINS_LIMIT`), each named once |
| `files` | 1 to 64 files (`FILES_LIMIT`), each with its `name`, its `size` in bytes, its `sha256`, and `from`, the address it is first asked for at |

(`catalogue::a_catalogue_is_read_as_its_models_and_their_files`,
`catalogue::an_entry_holds_a_bounded_count_of_features_each_a_name_declared_once`,
`catalogue::every_other_field_of_an_entry_keeps_its_rule`.) What an entry is made from,
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
- **Features, origins, files and models are kept in the order of their texts,** whatever
  order they were written in. Every such text is ASCII, so the order of its bytes and
  the order of its characters are one order; "the order of their names" and "the order
  of the bytes of their names" in this record mean that one.

**The file.** A catalogue is a file a person writes, in JSON, under the format
`catalogue/1`:

```json
{
  "format": "catalogue/1",
  "models": [
    {
      "name": "tiny",
      "revision": "2026-11.1",
      "adapter": "owlv2-sam2",
      "licence": "Apache-2.0",
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

1. **What the pointer does not name is deleted,** as below, and a pointer being
   written that a dead publication left (`CURRENT.new`) is removed.
2. **An identity that is on the disk already and whole is not downloaded again.**
   The folder of the identity being installed, when there is one, is read against
   the record: its `RECORD` byte for byte, its files each by size and hash, and
   nothing else in it. Whole, it is what would be staged, and the install goes to
   step 6 with it. Otherwise:
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
   `installations/<64 hex digits>`. Nothing names it yet.
6. `installations/` is synced, for a folder renamed in now and for one found
   whole: whoever renamed that one may have been refused this sync. So are the
   directory and `models/`, which name `models/` and the model's folder: a folder
   that was found made was made by one who may have been refused the sync of its
   name
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
  staging folder that is on another file system, by a link at `staging/`, makes the
  rename fail: that is a refusal, not damage
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
  start and at its end, and which every removal of the model will run (tasks M5b and
  M5c), over everything under `installations/` that the pointer does not name, but
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
  when it names it already. Damaged and not in use, it is deleted when its hold
  file is free, and the staged folder renamed in; held, the install is refused as
  busy
  (`publishing::an_identity_that_is_on_the_disk_already_is_pointed_to_when_whole_and_replaced_when_damaged`).
- **A repair has the identity of what it repairs,** and so the same folder. Under the
  model's lock, the damaged installation is removed first, as a removal removes it:
  under the directory's lock the draining mark is read and the pointer is cleared;
  the hold file is tried alone; and the folder is deleted. Only then is the staged
  folder renamed in and the pointer set. **In between the model is not installed.**
  With a hold on it the repair answers busy and the pointer is put back, whatever the
  mark says by then; with the mark set before the pointer is cleared, the repair is
  refused and nothing is changed
  (`publishing::a_repair_clears_the_pointer_deletes_the_damaged_installation_and_then_publishes`,
  `publishing::a_mark_of_draining_stops_the_publication_before_a_hold_file_or_a_pointer_is_made`).
  **The repair tries the hold file once.** Waiting for holds until a deadline needs
  the clock and the pacing of removal (task M5b), which may give the repair the same
  wait.

### The status record

Code (task M2b: `status.rs`; tests named `status::…`). `models/<model>/STATUS` is
what the installer last said: whether the model is being installed, how many of
the entry's files are staged, and how the last install ended. It is compact JSON
under the format `status/1`, in one form:

```json
{"format":"status/1","state":"installing","files_done":1,"files":2}
```

| Field | What it is |
|---|---|
| `state` | `installing`; or how the last install ended: `installed`, `failed`, `cancelled` |
| `files_done` | How many files were staged and held to their size and hash, when the record was written |
| `files` | How many files the entry has: 1 to 64 |

- **It is read strictly,** as an installation's record is ("How a record is read"):
  at most 256 bytes (`STATUS_MAX_BYTES`), refused over that before anything of it is
  looked at; an unknown field, a missing one, a value of another kind, a state that
  is none of the four, more files staged than there are, and any bytes but the one
  form are refused. It is read through the streamed read, so a link is not followed
  (`status::a_status_record_round_trips_in_one_form`,
  `status::a_status_counts_one_to_a_bounded_count_of_files_and_no_more_staged_than_there_are`,
  `status::a_status_record_is_read_strictly_and_refused_over_its_bound`,
  `publishing::the_status_says_installing_and_how_many_files_are_staged_while_the_install_goes`).
- **It is written by the holder of the model's lock:** when an install starts, after
  each file is staged, and when the install ends, however it ends. It is written
  whole beside the old one (`STATUS.new`) and renamed over it. An install that never
  had the lock (busy, a secret in a name, cancelled before it began) writes none.
- **After a refused sync of the pointer it says `failed` while the pointer names the
  new installation** ("Limits"): the install was refused, and it is the pointer that
  says what is installed. Asking again answers with the installation and says
  `installed`.
- **Nothing is decided from it, and it is given no sync of its own.** Whether an install is
  running is said by the model's lock; which installation is in use is said by the
  pointer. A crash can leave a status that says `installing` with no installer
  alive: a status that says so while the model's lock is free is the status of an
  install that died. A status that cannot be written fails an install that has not
  published, and changes nothing of one that has.
- **It holds no name and no path:** a state's word and two counts.
- It does not yet say whether the holder of the model's lock is an installer or a
  remover; tasks M3 and M5b add that ("For later tasks").

### Holds

*Decided, not built* (task M5). **A hold is a file lock, not a process id.** A hold on
one installation is a shared lock on its hold file, `holds/<64 hex digits>`, taken
under the directory's lock after the pointer was read and found to name that
installation with no draining mark set. **A hold is given or refused at once:** its share
of the hold file is a try that does not wait, and a hold file found held alone is a
hold refused. The store's interface gains that try with task M5: its shared call waits
today, and a holder waiting under the directory's lock would stall every model. So where the plan speaks of a waiting hold, here there is a hold that
was refused and may be asked for again. The operating system lets a lock go when its
process ends, however it ends, so a hold of a dead process is no hold; nothing reads a
process id from a file, and nothing is signalled by one (rascal's R3 and R5).

**Holds are counted inside a process** as well. A process's own shared lock on a hold
file stops its own try to take the file alone, as another process's would, and its
second share of the file is a second lock; so a process keeps one share of a hold file
and a count of its holds on it, and lets the share go when the count is nothing.

### Removal

*Decided, not built* (tasks M5b, M5d, M5c). In this order:

1. **The draining mark comes first.** It is set under the directory's lock, before the
   removal has the model's lock. From then no hold on that model is given, and no
   install of it publishes: an installer that holds the model's lock for a download
   reads the mark, between files and inside one, and stops, which lets the model's
   lock go.
2. The removal takes the model's lock alone, by trying until the same deadline as step
   4. Past it the removal answers busy and takes the mark off under the directory's
   lock. Having the lock, it sets the mark again if it is gone: whoever takes the
   model's lock cannot tell a dead remover's mark from a live one's, and takes it off.
3. Under the directory's lock, the pointer is cleared.
4. **The holds are waited for by trying, not by waiting:** the hold file is tried
   alone, without waiting, again and again until a deadline of the injected clock.
   The core does not sleep between tries; what paces them is given by the caller, and
   a test moves the clock by hand.
5. With the hold file alone, the installation's folder is deleted and
   `installations/` synced; the hold file is let go, and stays. The mark is taken off
   under the directory's lock. Past the deadline the removal answers busy: under the
   directory's lock the pointer is put back first, and then the mark is taken off, so
   that no reader and no crash finds the model with neither a pointer nor a mark.

A mark left by a process that died is taken off by the next to take the model's lock,
which also finishes a removal that had cleared the pointer (tasks M5d and M5c).

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
- **A link where one of the fixed folders or a lock file belongs is followed:**
  `models/`, a model's folder, `staging/`, `installations/`, `holds/` and a model's
  `LOCK` are made when they are not there and used when they are; a `LOCK` that is a
  link locks, or makes, the file it leads to, and so does the directory's. The
  store's interface cannot say whether what is there is a folder or a link to
  one. Whoever can put a link there can write the directory itself. The staging
  folder inside them is removed and made again each time, so a link there is not
  followed. **A link where an installation's folder belongs** is followed when a
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
- **The directory's lock is tried, not waited for,** a bounded count of times
  ("Three kinds of lock"). Under a holder that is slow to sync, a publication can be
  refused as busy with its installation renamed in and not named; the folder is
  left, and the next install of that identity points to it without a download. A
  repair refused so after it cleared the pointer leaves the model not installed. A
  lock taken alone that waits is not in the store's interface
  (`docs/carry-over.md`).
- **A reader that holds no lock does not read the files.** `current` answers from the
  pointer and the record; that the files are whole is held when they are published
  and when the same identity is published again.
- **A name that is one plain name on macOS and Linux may be none elsewhere.** A name
  that ends in a dot, and the names Windows keeps for devices (`con`, `nul`, `aux` and
  the like), are accepted: the platforms are macOS and Linux (`SPEC.md` section 1).
- **Not in `catalogue/1`:** a model's title, the platforms it runs on, and a credential
  for a download that needs one. Each comes with the first catalogue entry that needs
  it, at checkpoint 1C, and is on `docs/carry-over.md`.

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
  file free it deletes it and renames; with it damaged and held it answers busy. Under
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
- **M6 compares two revisions by equality only.** Nothing is newer or older.
- **M3 and M5d:** the status record says whether the holder of the model's lock is an
  installer or a remover, so that one who attaches knows what it is waiting for. As
  task M2b left it, `status/1` has no state for a remover: the task that writes one
  adds it. And staging asks the cancellation once, before it takes the lock: "nothing
  is asked after a cancel" needs it asked before each request and each move.
- **M5b:** the draining mark is read by a publication as there or not there
  (`DRAINING` in the model's folder), under the directory's lock. A mark with content
  is read the same way. A stale mark stops every publication of the model until the
  task that takes one off (M5d).
- **M5: the store's try for a share** (ADR 0018, `try_share`) is refused by the
  process's own lock on the file alone, and given beside its own shares as a second
  lock. A file system that answers that it cannot try is an error, never "held
  alone".
- **M5:** the count inside a process does not stand in for the check: every hold reads
  the pointer and the mark under the directory's lock, whatever the count. Letting the
  last hold go and taking a new one in another thread go through one guard inside the
  process, so that the share is not let go under a hold just counted. A hold on an
  installation whose hold file is missing is refused as damage; a hold never makes one.
- **M5b and M6:** a reader that holds no lock reads the mark as well as the pointer, and
  says "draining", not "not installed", of a model whose pointer a removal has cleared.
- **M5c:** a folder under `installations/` that the pointer does not name and that has
  no hold file was never published: it is deleted, and no hold file is made for it.
- **The crashes the tests of M2b and M5c hold,** each reopened by the next to take the
  model's lock. The first three rows and the last are held by the second pull request
  of task M2b, which names its tests here. "Step 1" in it is the rename of the staged
  folder and its sync (steps 5 and 6 of "Publishing"), and "step 2" the pointer (step
  7). Where a row says the next deletes a folder the pointer does not name, the next
  is a removal or an install of another identity: an install of the same identity
  points to the folder when it is whole, and deletes it when it is not:

  | A crash | What is on the disk | What the next does | The model is |
  |---|---|---|---|
  | Publishing, after step 1 | A folder the pointer does not name, with no hold file, or with one from an earlier publication of that identity | Deletes it, taking the hold file alone when there is one | As it was |
  | Publishing, inside step 2, before the pointer's rename is durable | The same, with the hold file made | Takes the hold file alone and deletes the folder | As it was |
  | Publishing, after the pointer's rename is durable | The new pointer; the old folder, superseded | Cleans the old folder when its hold file is free | The new installation |
  | Removal, after step 1 | The mark; the pointer as it was | Takes the mark off | Installed |
  | Removal, after step 3 | The mark; no pointer; the folder | Finishes the removal: deletes the folder, takes the mark off | Not installed |
  | Cleaning a superseded folder, or a repair, part-way through deleting | Part of a folder the pointer does not name | Deletes the rest | As the pointer says: for a repair, not installed |
- **M2 and M2b** make every name on a disk from a `ModelName`, a `FileName` or an
  `InstallationId`, and each plants a secret and searches every name and byte it wrote.
  A catalogue made by a constructor that was told of other secrets than the installer's
  is checked again by the installer.
- **M6** compares `catalogue_revision` with the entry's revision, and
  `adapter_revision` with the one the adapter states.
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
