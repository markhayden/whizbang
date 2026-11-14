# 0017. The models directory: a catalogue, an installation and its identity, a pointer, and holds

Status: accepted
Date: 2026-11-14
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

The code of this record that exists is `crates/whizbang-backends/src/models/`: `name.rs`
and `refused.rs`. Its tests are in `crates/whizbang-backends/tests/it/models/`, named
below as `names::…` and `refusals::…`.

**What is code now, and what is decided for later.** Each part below says which it is. A
part marked *decided, not built* has no code and no test yet, and names no test; what
builds it adds its tests' names here, and this record is what it is held to. Task M1
came to more lines than one pull request may hold (the plan, "Order"), so it is three
pull requests, and this record arrives with the first.

| Part | State |
|---|---|
| Names; what a refusal says | Code, with the tests named |
| The catalogue; how a record is read | Decided, not built: the second pull request of task M1 |
| An installation's record; its identity | Decided, not built: the third pull request of task M1 |
| The directory's layout and its three kinds of lock; publishing; the pointer; holds; removal | Decided, not built: tasks M2, M2b, M5 to M5c, M6 |

### The directory

*Decided, not built.* A models directory is one folder, given by whoever opens it: no
default place and no variable is read for it here. In it:

```text
<models>/
  LOCK                             the directory's lock
  models/
    <model>/                       one model, by its name in the catalogue
      LOCK                         the model's lock
      CURRENT                      the pointer: the identity of the installation in use
      STATUS                       what the installer last said (task M2b)
      DRAINING                     removal's mark (task M5b)
      staging/                     installs in progress (task M2)
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
- **An installation's folder is named by its identity,** as 64 hex digits (the hex of
  an `InstallationId`, which the third pull request of task M1 brings), and holds
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
- **The content of `STATUS` and `DRAINING`, and the names inside `staging/`,** are
  decided by the tasks that build them.

### Three kinds of lock, and their order

*Decided, not built.* All three are the locks of the store's file-system interface
(`whizbang_core::store::fs`), which are `flock` on Linux and macOS, and everything
`store/fs.rs` says of them holds here: a models directory on a network file system is
not supported, and **no lock file is ever removed or replaced:** not the directory's
`LOCK`, not a model's `LOCK`, not a file under `holds/`. There is no exception. The row
of `docs/carry-over.md` on those locks names the models directory too.

- **The directory's lock** (`LOCK`) is taken alone, for a moment, around every change of
  a pointer or of a draining mark, around the making of a hold file, and around the
  reading of the pointer and the mark that a hold or a publication decides from. It is
  never held across a download or while anything is waited for.
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
| Nothing | The directory's lock; a model's lock, alone or shared | |
| The directory's lock | Nothing | Takes no model's lock. Takes a hold file only to share it, as below, which does not wait. Waits for no hold |
| A model's lock, alone | The directory's lock | Waits for no hold file: it takes one alone by a try that does not wait, or not at all |
| A model's lock, shared (attached) | Nothing but the lock it shares | Takes no other lock |
| A hold | The directory's lock, to take another hold | Never waits for that model's lock, alone or shared: a run that holds a model neither installs it, removes it nor attaches to its install |

- **A hold is taken only under the directory's lock,** after the pointer was read and
  names that installation and no draining mark is set. Its share of the hold file does
  not wait there: a hold file is held alone only while its installation's folder is
  being deleted, by a holder of the model's lock, after the pointer stopped naming
  that installation; and the pointer cannot name it again until that holder has let
  the hold file go, which it does before it lets the model's lock go.
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
  refused as one name twice (the catalogue's check, in the second pull request).
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

*Decided, not built.* A `Catalogue` is the models that can be installed: at most 256
(`MODELS_LIMIT`), no two of one name, kept in the order of their names. An `Entry` is one model:

| Field | What it is |
|---|---|
| `name` | The model's name, a `ModelName` |
| `revision` | The revision of this entry, a `Revision`. Whoever changes anything of an entry changes its revision |
| `adapter` | The adapter that runs the model, by its name, a `ModelName` |
| `licence` | The licence the files are under, as their maker states it |
| `features` | The features the adapter gives with this model: at most 16 (`FEATURES_LIMIT`), each a feature's name as a backend declares one (ADR 0006), each stated once |
| `origins` | The origins a download of the files may reach: at most 8 (`ORIGINS_LIMIT`), each named once |
| `files` | 1 to 64 files (`FILES_LIMIT`), each with its `name`, its `size` in bytes, its `sha256`, and `from`, the address it is first asked for at |


- **A file's size** is 1 byte to 1 TiB (`FILE_MAX_BYTES`). It is what a download is
  stopped at, so a size of zero is refused: it would stop a download before its first
  byte and prove nothing.
- **A file's hash** is written as every digest is (ADR 0001): `sha256:` and 64 lower-case
  hex digits.
- **Where a file may be downloaded from is two things.** `from` is an `Address` (ADR
  0012): where the first request for the file goes. `origins` is every origin a download
  of this model's files may reach, the first address's and any it is moved to, since a
  download does not follow a redirect and hands back the address it was pointed to (ADR
  0012, "Three interfaces"). A file whose `from` is at an origin the entry does not name
  is refused, and `Entry::may_download_from` is the question the installer asks of each
  address it is moved to (task M2). An origin is written as an origin, with no path
  after it.
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
or not. What
whizbang writes of one (`Catalogue::to_bytes`) is written one way: compact, the fields
in the order above, and an address as its origin and the rest of it; a catalogue reads
back from that as itself.

**A catalogue that could not be read back is not made.** `Catalogue::new` refuses one
whose written form is over the bound, and one whose written form holds a secret. The written form is looked at whole, because it puts side by side
what a person wrote apart, and writes an address without a port that is its scheme's
own, which brings its host and its path together.
A part that holds a secret is refused where the part is made, a file's address and a
feature among them, and a secret that only the lowering of a host makes (ADR 0012) is
refused as a secret.

### How a record is read

*Decided, not built.* Stored bytes are not trusted, a catalogue's and an installation
record's alike. In this order:

1. **The length.** Bytes over the bound are refused as `Refused::TooLarge` before
   anything of them is looked at. Read from a source (`read_from`), no more than the
   bound and one byte is asked of it, however much it has; a source that fails is
   refused by the kind of its failure, with no path.
2. **A secret, in the bytes as they are.** Bytes that hold a secret the reader was told
   of, anywhere, are refused whole.
3. **JSON.** Bytes that are not JSON, that have anything after the one value, or that
   nest deeper than the format's reader follows (128 levels) are refused by their line.
4. **A field given twice** in one object, at any depth, is refused by its line. The usual
   reader keeps the last and says nothing.
5. **A secret, in every key and every text as JSON decoded it:** an escape can spell
   what the bytes do not hold.
6. **The fields, by hand.** Every object is an object with its fields by name; a field
   that is not one of its fields, a field that is missing, and a value of another kind
   than its field's are each refused, with the place.
   A size is a whole number from 0 to what 64 bits hold, written as one: a fraction, a
   negative number and a text are another kind. Every list's count is looked at before
   any of its items is. The format is the one the reader reads, or the record is refused.
7. **Every value by its rule,** through the same constructors that make a value in
   memory, so nothing is read that could not have been made.

### An installation and its record

*Decided, not built.* An **installation** is one model's files as one entry of a catalogue listed them,
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
  `InstallationRecord::of` beside the entry by whoever knows the adapter.
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
  `Refused::OtherForm`: the record is read, written again, and the two compared.
- **It is at most 32 KiB** (`INSTALLATION_MAX_BYTES`). The longest record there can be,
  every name at its limit and the most files of the largest size, is 15,979 bytes, so no
  record that can be made is refused when read back.
- **A record that would spell a secret as it is written is not made.**

### The identity of an installation

*Decided, not built.* An installation is named by `InstallationId`: the SHA-256 of one
encoding of its record, hashed as ADR 0001 hashes every identity ("How an identity is hashed"). The
kind is `installation/1`. The bytes hashed are:

- the bytes `whizbang:installation/1` and a zero byte;
- the model's name, the catalogue's revision, the adapter's name and the adapter's
  revision, in that order, each as bytes: the tag `b`, the length as eight bytes,
  big-endian, and the text's bytes;
- the count of files: the tag `u` and eight bytes, big-endian;
- for each file, in the order of the bytes of its name: its name (`b`, as above), its
  size (`u`), and its hash (the tag `d` and the 32 bytes of the digest).

It is written `sha256:` and 64 lower-case hex digits, and read back in that form and no
other.

**Three vectors,** computed apart from the code, which a reader can compute again from
the words above.
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
  its length, so no two records share one. **Two records that
  differ in the adapter's revision alone have two identities:**
  the second and third vectors.
- **Nothing local is in it:** no path, no time, no process, no address a file came
  from. The same entry installed under the same adapter's revision on two machines is
  one identity.
- **It is the version of a local model** in what a preparation saw (ADR 0004; the plan's
  task I7): a model's version is 1 to 128 printable ASCII characters (ADR 0006), and an
  identity's text is 71. So an installation replaced under a prepared call makes the
  preparation stale.
- The record is not hashed as its JSON. ADR 0001 turned that down for every identity:
  the bytes would depend on how a library writes JSON.

### The pointer

*Decided, not built* (task M2b). `models/<model>/CURRENT` names the installation in
use. Its content is the identity's text and one line feed, 72 bytes, and it is read
back in that form and no other. It is replaced by writing a new file and renaming it
over the old one, under the directory's lock. It names a whole, verified installation
or it is not there: a model with no `CURRENT` is not installed, whatever else its
folder holds.

**A reader that holds no lock** (the inventory, task M6) reads the pointer and then the
record it names. When the record is not there, it reads the pointer again before it
says anything: if the pointer now names another installation, or none, it answers from
that; it reports damage only when the pointer still names the installation whose
record is missing.

### Publishing

*Decided, not built* (task M2b). Under the model's lock, with the files staged and
verified:

1. The staged folder, with its `RECORD` and its `files/`, is renamed to
   `installations/<64 hex digits>`, and `installations/` is synced. Nothing names it
   yet.
2. Under the directory's lock: the draining mark is read, and with one set nothing
   more is published (the install stops, task M5d); the identity's hold file is made if
   it is not there, and `holds/` synced; the new pointer is written beside the old and
   renamed over it, and the model's folder synced.

A crash before the second rename is durable leaves the model as it was: the pointer
names what it named, or nothing. A folder under `installations/` that the pointer does
not name is not an installation in use, and is cleaned as below.

- **The installation the pointer named before is superseded,** and publishing does not
  delete it: a run may hold it. An installation is superseded when it is under
  `installations/` and the pointer does not name it; there is no mark for it. **It is
  deleted by removal's own routine** (tasks M5b and M5c), which every install and every
  removal of the model runs under the model's lock, at its start and after it
  publishes, over each superseded installation: it tries the hold file alone, once,
  without waiting; with it, it deletes the folder and syncs; without, it leaves the
  folder for the next install or removal. No new hold can arrive on a superseded
  installation, since a hold is given only on what the pointer names.
- **A repair has the identity of what it repairs,** and so the same folder. Under the
  model's lock, the damaged installation is removed first, as a removal removes it:
  the pointer is cleared, its holds are waited for until the deadline, and its folder
  is deleted. Only then is the staged folder renamed in and the pointer set. **In
  between the model is not installed.** With a hold kept past the deadline the repair
  answers busy and the pointer is put back.

### Holds

*Decided, not built* (task M5). **A hold is a file lock, not a process id.** A hold on
one installation is a shared lock on its hold file, `holds/<64 hex digits>`, taken
under the directory's lock after the pointer was read and found to name that
installation with no draining mark set. **A hold is given or refused at once:** nothing
waits for one, so where the plan speaks of a waiting hold, here there is a hold that
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
2. The removal takes the model's lock alone.
3. Under the directory's lock, the pointer is cleared.
4. **The holds are waited for by trying, not by waiting:** the hold file is tried
   alone, without waiting, again and again until a deadline of the injected clock.
   The core does not sleep between tries; what paces them is given by the caller, and
   a test moves the clock by hand.
5. With the hold file alone, the installation's folder is deleted and
   `installations/` synced; the hold file is let go, and stays. The mark is taken off
   under the directory's lock. Past the deadline the removal answers busy: under the
   directory's lock the pointer is put back and the mark taken off.

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
- **A megabyte of JSON may take some tens of megabytes while it is read:** a list of
  many small values in a field nobody knows is read before it is refused.
- **Names are ASCII.** A file whose maker named it with other letters is renamed in its
  catalogue entry.
- **A name that is one plain name on macOS and Linux may be none elsewhere.** A name
  that ends in a dot, and the names Windows keeps for devices (`con`, `nul`, `aux` and
  the like), are accepted: the platforms are macOS and Linux (`SPEC.md` section 1).
- **Not in `catalogue/1`:** a model's title, the platforms it runs on, and a credential
  for a download that needs one. Each comes with the first catalogue entry that needs
  it, at checkpoint 1C, and is on `docs/carry-over.md`.

### For later tasks

- **M2** is handed a `Downloader` (ADR 0012) and an `Entry`. Each request is for a
  file's `from` or for an address a download was moved to, and the second only when
  `Entry::may_download_from` answers yes for its origin. A file's `size` is the most
  bytes it asks for and what its sink stops at.
- **M2** names nothing in staging by adding a suffix or a prefix to a `FileName` in a
  folder that also holds plain file names: `a` with `.part` added is the plain name
  `a.part`. A file being downloaded has a folder of its own kind, or a name that no
  `FileName` can be.
- **M2b** names an installation's folder by the hex of its identity, writes `RECORD` as
  `InstallationRecord::to_bytes` and reads it back with `InstallationRecord::parse`. A
  record whose identity is not the name of its folder, or not what the pointer says, is
  damaged.
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
