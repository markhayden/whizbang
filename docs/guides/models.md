# Models

A local model is a set of files whizbang downloads once and keeps in a folder of its
own, the models directory. `whizbang models` lists the models this build knows, gets
one, and removes one. Why it is built this way is in
`docs/adr/0017-the-models-directory.md`. The playground's Models screen does the same from
the page (`docs/guides/playground.md`, "Models and keys").

**Today the catalogue holds two models, SAM 3** (`sam3`) **and Tiny** (`tiny`: OWLv2
finds boxes for a phrase and SAM 2.1 Hiera-Tiny draws each mask). The catalogue is the
list of models a build can install, and it is built into the binary
(`crates/whizbang/src/catalogue.json`; task N5 of checkpoint 1C,
`docs/plans/m1c-local-pack.md`, and task T1 of checkpoint 1D,
`docs/plans/m2b-checkpoint-1d.md`). On a Mac with nothing installed yet:

```text
$ whizbang models list
models: 2
directory: the one in this machine's data folder for whizbang, not made yet
sam3	SAM 3	SAM License	3.4 GB	macos-arm64	not installed
tiny	Tiny (OWLv2 and SAM 2.1 Hiera-Tiny)	Apache-2.0	777.4 MB	macos-arm64	not installed
```

**SAM 3 installs, and is ready once installed.** `get` installs its files, and `list` then
says `installed`: the pack has its adapter (`sam3`, task R2a of the same checkpoint), which
answers phrases on the Mac's GPU once `whizbang setup --extra torch` has installed its
runtime (`docs/guides/setup.md`). Task N5 fixed the adapter's revision before the adapter shipped, so a model
installed then is ready as it is, and nothing is downloaded again. A model whose adapter
a build lacks is listed as `installed, not ready: this build has no adapter for it`, and a
call that asks for it is refused before any model is loaded.
SAM 3's files are behind Hugging Face's sign-in: see "A model whose files need a
token" below. Its entry names `macos-arm64` alone; on Linux it is listed as not
running on this platform, and `get` refuses it.

**Tiny installs the same way, with no sign-in**: its two repositories are public
(Apache-2.0), so `whizbang models get tiny` asks for no token. It answers phrases,
points and boxes with the adapter `owlv2-sam2`, on the same runtime as SAM 3; it too
runs on the Mac alone. A file it shares with SAM 3 by its size and hash (the
tokenizer's `merges.txt`) is linked from SAM 3's installation when that is there, not
downloaded again.

The fast tests show every command with a catalogue of their own and a stand-in for the
download (`crates/whizbang/tests/it/models.rs`); nothing in them leaves the machine.

## The three commands

```text
whizbang models list   [--config <file> [--env-file <file>]] [--models-dir <dir>]
whizbang models get    <model> [--config <file> [--env-file <file>]] [--models-dir <dir>]
whizbang models remove <model> [--wait <seconds>] [--config <file> [--env-file <file>]] [--models-dir <dir>]
whizbang models verify <model> [--config <file> [--env-file <file>]] [--models-dir <dir>]
```

`verify` reads every file of the model's installation whole and holds it to its size and
hash, whatever an earlier check recorded, and prints `whole` and the installation's
identity; a file that is not whole is named as such, and `get` installs the model again.
A load trusts a file whose size, times and inode are as its last full check recorded, so
`verify` is the check to ask for when you doubt the disk.

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

**A file another installation holds is not downloaded.** When a file of the model, by
its size and its hash, is held already by any installation in the models directory, of
this model at another revision or of another model, `get` gives that file a second name
in the new installation, reads it back whole, and downloads nothing for it. The file is
on the disk once, and removing one of the installations leaves it to the other. Two
`get`s of models that share a file, run at once, download it once: the second waits
for the first and then links what it installed.

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
- a model whose adapter is not in this build's table of adapters, since it could not
  run here. An adapter the table names but the pack does not ship yet, as SAM 3's
  today, does not stop `get`: the model is installed under the revision the adapter
  will ship under.

A download that is slower than the config's `download_least_kib_per_second` over a
whole minute, 256 kibibytes a second unless the config says, is ended as timed out
(the config guide, "Where models are kept").

### A model whose files need a token

Some models' files are behind a sign-in: SAM 3's are on Hugging Face, which gives them
only to an account that was granted access. A catalogue entry says when a download from
one of its origins needs a token. SAM 3's entry says so of `https://huggingface.co`.

**Where `get` takes the token from is what the config says for that origin,** and
nowhere else (the config guide, "A download's token"). There are two ways, and an
origin is given one of them. The sign-in Hugging Face's own command-line tool keeps on
this machine:

```toml
[[models.tokens]]
origin = "https://huggingface.co"
sign_in = "huggingface"
```

Or a variable of the env file (`--env-file`), read as every key is:

```toml
[[models.tokens]]
origin = "https://huggingface.co"
key_env = "HF_DOWNLOAD_TOKEN"
```

- **The sign-in** is the token `hf auth login` writes. `get` reads it from where that tool
  keeps it: the file `HF_TOKEN_PATH` names when that variable is set; otherwise `token`
  in the folder `HF_HOME` names; otherwise `huggingface/token` in the folder
  `XDG_CACHE_HOME` names; otherwise `.cache/huggingface/token` in your home folder. A
  variable that names a place that is not an absolute path is not followed: the sign-in
  is then not found. The sign-in is taken for `https://huggingface.co` alone, so its
  token is never sent to another origin; a config that names it for any other is refused.
- **`HF_TOKEN` and `HUGGING_FACE_HUB_TOKEN` are not read.** Hugging Face's own tools
  take them in place of the file; whizbang takes the sign-in, or the env file's key.
- **The token file must be yours alone,** readable and writable by you and nobody else
  (mode 600), in a folder only you can write (700), which is how the tool writes both.
  One that others can read or write, one that is another user's, and one in a folder
  others can write are refused, not read: `chmod 600` the file and `chmod 700` its
  folder, or sign in again. Anything at the place that is not a regular file, a pipe
  say, is refused before it is opened. The file holds the token and at most one line
  break after it.
- **Nothing is read unless the config says so.** There is no default: without the
  lines above, `get` reads no sign-in, so nobody's token is sent without their saying
  so.

`get` reads the token only when it is to install a model whose entry says a download
from that origin needs one and a file is to be downloaded, and before anything is asked
or the models directory is made: a model whose every file another installation holds is
installed with no token at all; `list`, `remove`, and `get` of any other model never look at the sign-in. The
token is sent with every request to its origin, and with no request to another, also
when a server moves a file to another origin. It is in nothing `get` prints, in no
status record and in no refusal, and it is not kept after the install ends.

What `get` says when the config names no token, before anything is downloaded; the
last three lines are what to add to the config:

```text
whizbang: a download of the model from https://huggingface.co needs a token, and the config names none for that origin, so the model was not installed and nothing was downloaded. To use the sign-in of the Hub's command-line tool (`hf auth login`), add these three lines to the config; or name a variable of the env file that holds the token, as `key_env` in place of `sign_in`:
[[models.tokens]]
origin = "https://huggingface.co"
sign_in = "huggingface"
```

A sign-in that is not there or cannot be read is said the same way, with what is wrong
and never the file's path or what it holds. When the origin refuses the token, `get`
says to ask for access to the model on its page with the account the token is of (on
Hugging Face, by accepting the model's terms there).

### A new revision of SAM 3: points and boxes

The SAM 3 of this build answers a point and a box as well as a phrase: a new adapter
of the pack, under a new revision of the catalogue's entry, with the same files. Two
commands bring a machine that installed the one before up to it, and neither downloads
the weights again or asks for a token:

```text
$ whizbang setup
$ whizbang models get sam3
installed sam3: sha256:…
```

`setup` writes the pack's new copy and keeps the runtime `setup --extra torch`
installed; `get` finds every file held by the installation before, links them, and
fetches nothing.

A cut-out with a point is then three commands: `segment` asks for the thing at a place
and prints its candidates, best first; `select` keeps one of them by its place in that
list; `render` makes the cut-out. The run's id and each mask's identity are what
`segment` prints:

```text
$ whizbang rascal segment --image sha256:… --point "420 200" --store shop --config whizbang.toml
run … completed
mask sha256:… score 0.992 box 330 110 510 290
mask sha256:… score 0.316 box 80 111 510 440
mask sha256:… score 0.241 box 351 132 504 289
$ whizbang rascal select --run … --pick 1 --store shop
$ whizbang rascal render --mask sha256:… --profile alpha --image sha256:… --store shop
```

`--box "L T R B"` asks for the thing in a box in the same way, and `select --at "X Y"`
keeps the masks whose core holds a pixel, in place of `--pick`.

### The licences, and what a publication owes

`list` names each model's licence, and `get` installs its text beside the model's files.

- **SAM 3** is under the SAM License, which its maker publishes. You accept its terms on
  the model's page on Hugging Face, with your own sign-in, before its files can be
  downloaded ("A model whose files need a token", above); whizbang hosts no copy of them.
  `get` keeps the copy of the licence the catalogue pins, `LICENSE`, beside the weights,
  held to its size and hash like every file.
- **Tiny** is under Apache-2.0, which each model card states; the two cards are
  installed as `owlv2.README.md` and `sam2.README.md`.

**What a publication owes.** SAM 3's licence asks whoever submits for publication the
results of research done on, with or in connection with the model to acknowledge its use
in that publication (section 1.b.ii). That falls on whoever publishes, not on whizbang: a
run's record names the model of each request it made (its `models` list), which is
where to find what to cite. The copy in `LICENSE` holds the rest of its terms;
this guide is not legal advice.

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

## A model of your own

Two things can be added to a models directory beside what this build ships.

**Your own weights for an adapter whizbang has.** Write `catalogue.json` in the models
directory: a catalogue (`catalogue/2`) of your models, each entry as the shipped ones
are, naming an adapter whizbang ships (`sam3`, `owlv2-sam2`). Approve each origin it
downloads from in the config, as a recipient (`[egress.recipients.<name>]` with its
`origin` and `classes = []`), then `whizbang models get <model>` and configure a backend
of the kind `pack` for it. A model of the shipped catalogue cannot be named again there.

**Your own adapter.** Write one Python file whose class `ADAPTER` is the adapter (the
interface of `whizbang_pack/adapter.py`), and check it against the files of your model:

```sh
python -m whizbang_pack.conformance my_adapter.py <folder of the model's files>
whizbang models adapter add my_adapter.py --name mine
```

The first runs the conformance suite: each feature the adapter declares alone, then all
together, each answer held to the protocol's contract; it ends 0 when every run conforms.
The second copies the file into the models directory, read-only and named by its SHA-256,
as the adapter `user-mine`, which an entry of your `catalogue.json` may name. The file is
your code and runs in the worker: only for a model installed under that hash, and only
after the copy's bytes are checked against it. The check says which code runs, not what
it may do: it runs with the worker's full privileges, its open descriptors, its
environment, your files and the network, with nothing around it. Add only an adapter you
would run by hand. Adding a changed file under the same name
makes a new hash; `get` installs the model again under it, and a run already admitted
keeps the earlier copy. `whizbang setup` leaves both alone.

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
holds. With the catalogue this build ships, `list` and `remove` of `sam3` look for it
and make nothing; `get sam3` makes it. A command that runs an action, such as `whizbang
rascal grow`, opens it, and makes it where it is not, only under a config that has a
local backend: one whose table names no `url`, as a backend of the kind `pack` is.

A backend of the kind `pack` runs an installed model in the pack's worker, which
`whizbang setup` installs into the same directory:

```toml
[backends.segment]
kind = "pack"
model = "sam3"

[backends.segment.settings]
threshold = "0.2"
max_lifetime_s = 3600
```

Its settings are the model's, `threshold`, `pad` and `min_cover` (each a text of a
number from 0 to 1) and `outline` (a text), and its own: `call_limit_s`, the most seconds
one call may take, from 1 to 120 (120 when not given), and `max_lifetime_s`, the most
seconds the worker lives, from 1 to 31,536,000 (a day when not given). Any other setting
is refused, and the backend is not ready, with the reason.

Where the directory is, and the least rate of a download, are not part of what a run
means: moving the directory changes no config's digest, so a run sent again under its
request id is the same run.

## Writing a catalogue entry: `pin`

For whoever writes or changes an entry of the catalogue, not for using a model:

```text
whizbang models pin <draft> [--config <file> [--env-file <file>]]
```

The draft is a catalogue file (`catalogue/2`) of one entry whose sizes and hashes are
not known yet: each file's `size` is the most bytes wanted of it, and its `sha256` may
be anything of the right form. `pin` fetches each file once, through the same checked
download as `get`, held to the draft's origins and carrying the draft's token as `get`
would, and prints a line of each file's name, size and SHA-256. It installs nothing and
writes nothing: the bytes are hashed as they come.

A file that is moved to an origin the draft does not name is not fetched there; its
line names that origin, and only the origin, so the host can be looked at before an
entry lists it. A file that does not end within its bound, or whose download fails,
says so in its line, and `pin` then exits 1.

What `pin` prints is what the origin served that once: trust on first use. The shipped
entry's hashes were fixed this way, as its documentation (`wiring::catalogue`) and ADR
0017, "The shipped catalogue", say.

## Limits

- **A host's name is resolved by the system's resolver, with no limit of whizbang's
  own.** The download client asks the system to resolve the address's host (ADR 0024),
  and that one wait is bounded by the resolver's own settings: by default 5 seconds
  for a name server before the next is tried, with back-off, and 2 tries of each, of at
  most 3 name servers (`resolver(5)` and `<resolv.h>` on macOS, `resolv.conf(5)` on
  Linux). A machine's configuration can lengthen it. Every wait after it has a limit
  of whizbang's: 10 seconds to connect, 30 between two reads, and the least rate. If
  `get` hangs before the first byte, Ctrl-C ends it; nothing is left half installed.
- **A model dropped from the catalogue** is not removed by `remove`, which takes a name
  the catalogue holds. Its folder, `models/<name>` in the models directory, can be
  deleted by hand when no run is using it.
- **Ctrl-C during `get`** ends it. What it had downloaded is deleted by the next `get`
  of the model; the installation that was there before stays in use.
- **A `get` that attached to another install waits with no limit of its own.** It
  ends when the install it waits for ends; an install that never ends, a program stopped
  by a debugger say, keeps it waiting until Ctrl-C.
- **A build for a platform no catalogue names runs none of its models.** The catalogue
  names `macos-arm64` and `linux-x86_64`; a build for any other, Intel macOS for one,
  lists every model as not ready because it does not run on this platform, and `get`
  refuses each.
- **`get` of a model installed already reads its token too,** when it needs one:
  whether anything is to be downloaded is known only once the install has begun.
