# The config file

One TOML file says which backend answers each capability, what one call may spend, and
who may receive data. This page is how to write it. Why it is this way, and exactly how
its digest is made, is in `docs/adr/0011-the-config-file.md`.

Every example on this page is read by the fast tier: a block that is shown as read is
read, and a block that is shown as refused is refused with the sentence under it
(`config::examples::every_example_in_the_guide_and_in_the_record_is_read`,
`config::examples::the_guides_env_file_is_read_with_its_config`).

Nothing uses a config yet. The readers of the config and of the env file are built
(checkpoint 1B, tasks C1 and C2), and so is what a config offers with the kinds of
backend a build has, which in the shipped binary is none; the commands that take a
config, and everything it switches on, come with the tasks after. There is no default place for the file: it is
the one a command is given, and a command given none has no backend.

## Backends

One table for each capability that has a backend: `segment`, `fill`, `judge`.

```toml
[backends.segment]
kind = "remote"
url = "https://example-host/whizbang-sam3"
token_env = "SEGMENT_TOKEN"
model = "sam3"

[backends.fill]
kind = "service"
provider = "fal"
model = "bria-erase"
key_env = "FAL_KEY"
```

Written this way a capability has one backend. It is the default, the one that answers a
call that names none, and its name is `default`.

To give a capability several, put each in a table under a name of its own, and say which
is the default:

```toml
[backends.fill]
default = "bria"

[backends.fill.bria]
kind = "service"
provider = "fal"
model = "bria-erase"
key_env = "FAL_KEY"

[backends.fill.local]
kind = "pack"
model = "lama"

[backends.fill.local.settings]
longest_side = 2048
tiled = true
rendering = "grey"
```

A call chooses `bria` or `local` by name. Leave `default` out and a call must choose: no
backend is picked for it, however few there are. A name is a lower-case letter, then
lower-case letters, digits, `_` or `-`. Eight names are taken, since they are the keys of
these tables, and no backend can have one: `default`, `kind`, `model`, `provider`, `url`,
`key_env`, `token_env` and `settings`.

| Key | What it is |
|---|---|
| `kind` | Which code answers. Always given |
| `model` | The model to ask for |
| `provider` | The service it is reached through |
| `url` | Its address: `http://` or `https://`, a host in lower case, perhaps a port and a path |
| `key_env`, `token_env` | The name of the variable its key, or its token, is in |
| `settings` | The kind's own settings: texts, whole numbers, and true or false. A run records each under the backend's name and the setting's key, so the two together are 101 characters at most for `segment`, 103 for `judge` and 104 for `fill` |

## Keys are named, never written

A backend says which variable its key is in. The key itself goes in the env file, never
here. What is written where a variable's name belongs is not shown back, in case it is
the key:

```toml refused
[backends.fill]
kind = "service"
key_env = "not-a-variable"
```

```text
the config's key `backends.fill.key_env` must name a variable: capital letters, digits
and _, not a digit first, 64 characters at most; what is written there is not shown
```

An address has no place for one either: no user, no password, no query.

```toml refused
[backends.segment]
kind = "remote"
url = "https://example-host/whizbang-sam3?token=abc"
```

```text
the config's key `backends.segment.url` is refused: the address has a query, and may
have none; what is written there is not shown
```

And once the env file is read, a config that holds one of its values anywhere, a comment
included, is refused whole, and nothing of it is shown.

## The env file

The keys themselves are in an env file beside your config, which git should ignore:

```env
# Keys. Never commit this file.
FAL_KEY='made-up-key-0123456789'
export SEGMENT_TOKEN=made-up-token-0123456789
```

A line is `NAME=value`, with `export` before it if you like; quotes around a value are
taken off, and what is inside them is the value, all of it: a space at either end inside
the quotes is refused, and so are quotes around nothing. A name with nothing after the
`=`, and no quotes, is as a variable not set. A `#` straight after a value,
with no space before it, is part of the value; a comment has a line of its own. Save the
file as plain UTF-8 with no byte-order mark, which some editors add and which is refused.
A link to a file is followed, and what it leads to is read. Nothing is expanded, and the
environment of your shell is not read: a key is in this file or it is not set. Only the variables your config names are used, each by
the backend that names it. A backend that names a variable this file does not set is
not ready, and a call that would use it is told which setting, `key_env` or
`token_env`, names it. A command that will
use a backend that names a variable asks for the env file before it reads anything but
the config, naming the backend and `key_env` or `token_env`, never the variable; a
command that uses no such backend (the operations of `rascal`, `models list`, a
`rascal segment` on a local segmenter) runs without it. A key shorter than 8 bytes is refused, by the name of its
variable. Keep nothing but keys here: any value of 8 bytes or more is treated as a
secret, and a config that holds the same text is refused.

## What one call may spend

```toml
[spend]
call_cap_usd = 2.50
```

The most one call may reserve, in US dollars: more than 0, at most 1,000,000, and no
finer than a millionth. A call may ask for less and never for more. What a store may
spend in all is not in this file: it is set by `whizbang spend reset`, by the owner.

## Who may receive data

This table is what will be enforced once the gate is built: nothing will leave the
machine to a recipient it does not name, and a recipient will receive only the classes of
data listed for it. Today it is read and checked, and nothing sends anything.

```toml
[egress.recipients.fal]
origin = "https://fal.example"
account = "team-1234"
upstreams = ["https://storage.fal.example"]
classes = ["prompts", "source_images", "derived_images"]
probe = true

[egress.recipients.judge-host]
origin = "http://127.0.0.1:8080"
classes = []
```

| Key | What it is |
|---|---|
| `origin` | Where data is sent: a scheme, a host and perhaps a port, and nothing after it. Always given. `https://h.example`, `https://h.example:443` and `HTTPS://H.Example` are one origin |
| `account` | The account used with it: one word, with no space |
| `upstreams` | The origins it may pass data on to |
| `classes` | What it may receive, of `source_images`, `derived_images`, `prompts` and `evidence`. Always given; `[]` is nothing |
| `probe` | Whether it may be asked if it is reachable. Not unless this says `true` |

## Where models are kept

```toml
[models]
dir = "/srv/whizbang/models"
download_least_kib_per_second = 256
```

Both keys may be left out. `dir` is the models directory, where installed models are
kept. A command's `--models-dir` is asked before it; with neither, the directory is
the one in your machine's data folder for whizbang: `Library/Application
Support/whizbang/models` under your home folder on macOS, and
`$XDG_DATA_HOME/whizbang/models` on Linux, or `.local/share/whizbang/models` under
your home folder when that variable is not set. Whichever it is, it must be yours
alone to write: a folder that your group or anyone can write, or that is another
user's, is refused, and the refusal says how to mend it. The folder itself is made
when it is not there; the folder that is to hold it must be there, unless it is the
one in your data folder.

`dir` is an absolute path. A relative one would be another folder from each place you
run a command in, and `~` is not read as your home folder:

```toml refused
[models]
dir = "models"
```

```text
the config's key `models.dir` must be an absolute path: it starts with / and holds no
control character; a ~ is not read as a home folder; what is written there is not
shown
```

So is a command's `--models-dir`: a relative one is refused before anything is read or
made.

`download_least_kib_per_second` is the least rate a download of a model must keep up
over any whole minute, in kibibytes a second, from 1 to 1048576; a slower one is ended
as timed out. It is 256 when you do not say. `whizbang models get` is what downloads
(the guide `models.md`).

Neither key is in the config's digest: where models are kept, and how slow a download
may be, are not part of what a run means, so moving the directory makes no run of
yours another.

### A download's token

A model whose files are behind a sign-in, as SAM 3's are on Hugging Face, is downloaded
with a token. The config says where the token of an origin is, never the token itself:

```toml
[[models.tokens]]
origin = "https://huggingface.co"
sign_in = "huggingface"

[[models.tokens]]
origin = "https://weights.example.org"
key_env = "WEIGHTS_TOKEN"
```

Each entry is an `origin` and exactly one of two keys. `sign_in = "huggingface"` is the
sign-in Hugging Face's command-line tool keeps on this machine (`hf auth login`), which
`whizbang models get` reads only when a download from that origin needs it. `key_env`
names a variable of the env file, as a backend's key does, and a `get` that reads it
then needs the env file. Eight origins at most, each once. Like the other keys of `[models]`, these
are in no digest.

The sign-in's token is sent to Hugging Face and to no other origin, so it is taken for
`https://huggingface.co` alone:

```toml refused
[[models.tokens]]
origin = "https://weights.example.org"
sign_in = "huggingface"
```

```text
the config's key `models.tokens.0.sign_in` is taken for the origin
https://huggingface.co alone: the Hub's token is never sent to another
```

Where the sign-in keeps its token, and what `get` does with it, are in the guide
`models.md`.

## Checking an edit with a judge

`whizbang squint check <use-case>` asks a judge about a picture under one of five use
cases: `source-fidelity`, whether an edit kept what mattered in its source;
`reference-consistency`, whether a subject of the references appears faithfully;
`reality-checks`, mistakes no real photograph would make; `brief-compliance`, whether it
holds what the brief asked for and nothing it forbade; and `product-fidelity`, whether a
product shot keeps the exact product. The judge is a backend of the kind `service`, whose one
provider is Anthropic; the config names its model and the variable its key is in, caps
what one call may reserve, and approves the provider for the four classes of data a
check sends (plan 2A, "What the owner provides (S)"):

```toml
[backends.judge]
kind = "service"
provider = "anthropic"
model = "claude-haiku-5-5"
key_env = "ANTHROPIC_API_KEY"

[spend]
call_cap_usd = 2.00

[egress.recipients.anthropic]
origin = "https://api.anthropic.com"
classes = ["source_images", "derived_images", "prompts", "evidence"]
```

The key is in the env file, as `ANTHROPIC_API_KEY=…`, and the command is given both:

```text
whizbang squint check source-fidelity --source <id> --candidate <id> \
  --store <dir> --config whizbang.toml --env-file whizbang.env \
  --brief "change only the sky; the bird's tail keeps its colour"
```

`--brief` says what the edit was to do and what it had to keep. What it names as kept
and the edit changed fails the check; with no brief, nothing is named as kept, so a
change is noted in the reason but does not fail it.

The other use cases take the pictures their rules need, each by its identity in the
store: `--candidate` always; `--source` for `source-fidelity` alone; `--reference`, up to
eight times, for `reference-consistency` and `product-fidelity`, each labelled by its
place (`reference 1`, `reference 2`, …); and `--brief` for `brief-compliance`, which is
refused without one. A role a use case does not take is refused before anything is sent:

```text
whizbang squint check product-fidelity --candidate <id> --reference <id> --reference <id> \
  --store <dir> --config whizbang.toml --env-file whizbang.env
whizbang squint check brief-compliance --candidate <id> \
  --brief "a red square on grey; no text" --store <dir> --config whizbang.toml \
  --env-file whizbang.env
```

`--job <file>` takes a squint job file in place of those flags: its brief, its
requirements with their ids, and its images with their roles and labels. Each picture is
brought into the store from the file the job names, relative to the job file's folder,
and refused when its SHA-256 is not the one the job pins (ADR 0028).

**What one request may cost** is fixed by three settings of the backend, each held
before a byte is sent: `max_input_tokens` (40,000 when none is given),
`max_output_tokens` (2,048) and `max_pictures` (24), in a table
`[backends.judge.settings]`. A request is bound at its ceilings: 40,000 input tokens at
0.125 dollars a million and 2,048 output tokens at 0.50, which is 0.006024 dollars; a
check reserves that for each of its 16 turns, 0.096384 dollars, within the cap of 2.00.
Each turn is then settled at what its answer says it used. A conversation that needs
more raises the settings, and the cap with them (ADR 0026).

## What is refused

The file is read strictly, so that a slip is told and not guessed at. A refusal names
the key and never shows the value.

A key that is not known:

```toml refused
[backends.fill]
kind = "service"
modle = "bria-erase"
```

```text
the config's key `backends.fill.modle` is not known; the keys here are kind, model,
provider, url, key_env, token_env and settings
```

A value of the wrong kind:

```toml refused
[spend]
call_cap_usd = "2.50"
```

```text
the config's key `spend.call_cap_usd` must be a number, and is a text
```

A key given twice:

```toml refused
[spend]
call_cap_usd = 2
call_cap_usd = 3
```

```text
the config gives the key `call_cap_usd` twice, the second time at line 3
```

Also refused: a path that names a folder or anything else that is not a file; a file
over 256 KiB; bytes that are not UTF-8, or a control character; a
table `tools`, which has no shape until milestone 2; a key in the table `models`
that is not one of its two; and a text with a letter outside ASCII, outside a comment
and outside the models directory's path.

## Two files that say the same

A config has a digest, which a run records. It is of what the file says, not of its
bytes: comments, spacing, the order of tables and keys, and the ways TOML has of writing
one value change nothing. Any change to a setting changes it.
