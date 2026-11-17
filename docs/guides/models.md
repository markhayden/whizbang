# Models

A local model is a set of files whizbang downloads once and keeps in a folder of its
own, the models directory. `whizbang models` lists the models this build knows, gets
one, and removes one. Why it is built this way is in
`docs/adr/0017-the-models-directory.md`.

**Today the catalogue is empty.** The catalogue is the list of models a build can
install, and it is built into the binary. The first entry, SAM 3, arrives with task
N5 of checkpoint 1C (`docs/plans/m1c-local-pack.md`). Until then `list` says the
catalogue holds no model, and `get` and `remove` refuse every name:

```text
$ whizbang models list
models: 0: the catalogue holds no model yet
$ whizbang models get sam3
whizbang: the catalogue holds no model of that name; `whizbang models list` names those it holds
```

Everything below is what the commands do with a catalogue that holds models. The fast
tests show each of it with a catalogue of their own and a stand-in for the download
(`crates/whizbang/tests/it/models.rs`); nothing in them leaves the machine.

## The three commands

```text
whizbang models list   [--config <file> [--env-file <file>]] [--models-dir <dir>]
whizbang models get    <model> [--config <file> [--env-file <file>]] [--models-dir <dir>]
whizbang models remove <model> [--wait <seconds>] [--config <file> [--env-file <file>]] [--models-dir <dir>]
```

Each exits 0 when it did what was asked, 1 with one line on the standard error that
says why when it did not, and 2 when the command line was not understood.

### `list`

One line of how many models the catalogue holds, one of which models directory was
looked in, and then a line for each model, its fields apart by tabs: its name, its
title, its licence, the size of its files, the platforms it runs on, and its state.

```text
models: 2
directory: the one --models-dir names
alpha	Alpha, for tests	Apache-2.0	2.5 KB	linux-x86_64,macos-arm64	installed
beta	Beta, for tests	MIT	1.2 MB	linux-x86_64,macos-arm64	not installed
```

The directory is named by which of three said where it is, never by its path. When
nothing is there yet, the line says `not made yet`, every model is not installed, and
`list` makes nothing.

A model's state is one of these:

| State | What it means, and what to do |
|---|---|
| `installed` | It is installed and ready |
| `not installed` | `get` installs it |
| `being removed` | A `remove` of it is running now. A removal that was killed leaves no such state behind |
| `installed, not ready: …` or `not installed; not ready: …` | No install by this build would make it ready, and the rest says why: `this build has no adapter for it`, or `it does not run on this platform` |
| `installed, not ready: its installation is out of date; get installs the current one` | It was installed from another revision of its entry or of its adapter |
| `damaged: get installs it again` | What is on the disk is not a whole installation |
| `a folder of its layout is a link, which whizbang does not go through; take the link away` | One of the model's folders, or the folder that holds every model's folder, is a link. Whizbang lists, deletes and installs nothing through it |
| Anything else | Something of the model's folder could not be read, said in a few words |

### `get`

Installs the model, and prints the identity of its installation:

```text
$ whizbang models get alpha
installed alpha: sha256:…
```

The files are downloaded from the addresses the catalogue names, and from no origin
but those its entry names, even when a server points elsewhere; each is checked
against the size and the hash the catalogue states before anything is kept. A model that is installed already, whole, is not downloaded again.

When another `get` of the same model is running, from this machine, in another
terminal or by another program, this one downloads nothing: it waits for that one,
says how far it is each time that changes, and ends as it ends.

```text
whizbang: another request is installing the model: 1 of 2 files staged
installed alpha: sha256:…
```

It looks a quarter of a second at a time. Its waiting has no limit of its own: the
install it waits for ends, by finishing or by its download's own limits, and Ctrl-C
ends the one that waits without stopping the install.

`get` refuses, before anything is downloaded or made:

- a name the catalogue does not hold;
- a model whose entry does not name the platform this build is for, and says which
  platforms it names;
- a model whose adapter this build does not have, since it could not run here.

A download that is slower than the config's `download_least_kib_per_second` over a
whole minute, 256 kibibytes a second unless the config says, is ended as timed out
(the config guide, "Where models are kept").

**A model whose files need a token.** Some models' files are behind a sign-in: SAM 3's
are on Hugging Face, which gives them only to an account that was granted access. No
model in the catalogue needs one today, and `get` reads no token. With task N4 of
checkpoint 1C the config will say where the token of each origin is, either the sign-in
Hugging Face's own command-line tool keeps on the machine or a variable of the env file,
and `get` will read it only when a download from that origin needs it, and send it to
that origin only.

### `remove`

```text
$ whizbang models remove alpha --wait 60
removed alpha: sha256:…
```

A run that is using the model keeps it: from the moment `remove` starts, no new run is
given the model, and `remove` waits for the runs that have it, up to `--wait` seconds
(30 when it is not given, a day at most). Past that, nothing is removed, the model is
as it was, and it says so:

```text
whizbang: the model was not removed: the model is held by a run, which did not let it go in time; nothing was removed, and the model is as it was
```

Killed while it waits, it leaves the model installed and stops nobody. `remove` of a
model that is not installed says so, and makes nothing.

## Where the models directory is

Three may say, in this order: the command's `--models-dir`, which must be an absolute
path; the config's `[models] dir`, also absolute; and the folder this machine keeps
whizbang's data in, `Library/Application Support/whizbang/models` under your home folder
on macOS, and `$XDG_DATA_HOME/whizbang/models` on Linux, or
`.local/share/whizbang/models` under your home folder when that variable is not set. A
relative `--models-dir` is refused before anything is read or made.

The directory must be yours alone to write: one your group or anyone can write, or
another user's, is refused, and the refusal says how to mend it. `get` makes it when it
is not there, and the folders above it only when it is the one in the data folder.

**When a command looks for it at all.** Only when the catalogue gives it a reason:
`list` when the catalogue holds a model; `get` of a model the catalogue holds, that runs
on this platform and whose adapter this build has; `remove` of a model the catalogue
holds. So with the catalogue this build ships, which is empty, none of the three works
out where the directory is or looks at it.

Where the directory is, and the least rate of a download, are not part of what a run
means: moving the directory changes no config's digest, so a run sent again under its
request id is the same run.

## Limits

- **A host's name is resolved with no limit of whizbang's own.** The download client
  asks the system to resolve the address's host, and that one wait is bounded only by
  the system's resolver (ADR 0024; `docs/carry-over.md` has the row). Every wait after
  it has a limit: 10 seconds to
  connect, 30 between two reads, and the least rate. If `get` hangs before the first
  byte, Ctrl-C ends it; nothing is left half installed.
- **A model dropped from the catalogue** is not removed by `remove`, which takes a name
  the catalogue holds. Its folder, `models/<name>` in the models directory, can be
  deleted by hand when no run is using it.
- **Ctrl-C during `get`** ends it. What it had downloaded is deleted by the next `get`
  of the model; the installation that was there before stays in use.
