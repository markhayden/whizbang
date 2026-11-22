# The config file

One TOML file says which backend answers each capability, what one call may spend, and
who may receive data. This page is how to write it. Why it is this way, and exactly how
its digest is made, is in `docs/adr/0011-the-config-file.md`.

The playground (`whizbang play`, `docs/guides/playground.md`) reads this file and its env
file, as every command does, and never writes either: its Models screen says whether each
backend's key is set, and never shows, takes or stores a key.

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
model = "bria-eraser"
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
model = "bria-eraser"
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

## Answering a job asked again

```toml
[cache]
```

With the table `[cache]`, `whizbang squint check` and `whizbang serve` answer a check
asked again unchanged from the run that answered it first, and send nothing: the same use
case, brief, images (by their pixels) with their roles and order, rules, tools, judge and
config. Only a judge whose every answer named the model that gave it is cached. The run
that answers so is recorded, and names the earlier one. `decisions = false` in the table
turns it off; without the table it is off. Evaluation never reads it (ADR 0031).

## Tool files

```toml
[tools]
paths = ["/opt/tools/per_subject.py"]
```

Each path is a tool file of yours: a Python file that declares, in a block of comments at
its top, its name, the images it reads, what it measures with each measurement's bounds,
and its arguments, and has one function, `run(context)` (ADR 0032 gives the whole shape;
`pack/tools/per_subject.py` is one, and `whizbang setup` puts a copy of it at
`pack/project/tools/per_subject.py` in the models directory). At most 16, each an absolute path, each named once. A
file that cannot be read, that its group or others can write, or whose declaration lacks a
bound, is refused when the config is read. `whizbang try` and `whizbang eval tool` run it
by its name; `whizbang squint check --tool <name>` offers it to the judge; nothing offers it
otherwise. Each run uses the file as it is then, records its digest, and an edit is the next
run's: nothing is rebuilt or installed. A tool file runs in the tool worker, a second
process of the pack's runtime that `whizbang setup` installed, so the config needs a local
backend for whizbang to open the models directory it is in.

**A tool file is your code, and runs with your privileges.** Whizbang gives it no key, no
token and no home of yours, and reaches it only through its context, and it holds what
comes back to the declaration's bounds; but nothing stops the file from reading your files,
using the network or starting processes. A process it starts is ended with the worker,
unless it leaves the worker's process group. List only files you wrote or read.

## Who may receive data

This table is enforced: nothing leaves the machine to a recipient it does not name, and a
recipient receives only the classes of data listed for it. Each run records where its data
went.

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
product shot keeps the exact product. The judge is a backend of the kind `service`, whose
provider is Anthropic, Google or OpenAI; the config names its model and the variable its
key is in, caps what one call may reserve, and approves the provider for the four
classes of data a check sends (plan 2A, "What the owner provides (S)"). For Anthropic:

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

For Google, whose key goes in the header `x-goog-api-key` and never in the address:

```toml
[backends.judge]
kind = "service"
provider = "google"
model = "gemini-3.8-flash"
key_env = "GEMINI_API_KEY"

[spend]
call_cap_usd = 2.00

[egress.recipients.google]
origin = "https://generativelanguage.googleapis.com"
classes = ["source_images", "derived_images", "prompts", "evidence"]
```

For OpenAI, whose key goes in the header `Authorization`, as `Bearer` and the key:

```toml
[backends.judge]
kind = "service"
provider = "openai"
model = "gpt-6.1-terra"
key_env = "OPENAI_API_KEY"

[spend]
call_cap_usd = 2.00

[egress.recipients.openai]
origin = "https://api.openai.com"
classes = ["source_images", "derived_images", "prompts", "evidence"]
```

The models this build has prices for are `claude-haiku-5-5`; `gemini-3.8-flash` and
`gemini-4-argon`; and `gpt-6.1-terra` and `gpt-6.1-sol`. The setting `effort` takes
`low`, `medium`, `high`, `xhigh` or `max` for Anthropic; `low`, `medium` or `high`
(Gemini's thinking level) for Google; and `none`, `low`, `medium`, `high`, `xhigh` or
`max` for OpenAI. An OpenAI request is sent with `store` false, so the provider keeps
none of it between turns. A Google judge whose table sets no `effort` asks for `low`:
Gemini counts its thinking against the output limit, and left to its own level it spent
whole turns thinking. A higher level wants `max_output_tokens` raised with it.

The key is in the env file, as `ANTHROPIC_API_KEY=…` (or `GEMINI_API_KEY=…`,
`OPENAI_API_KEY=…`), and the command is given both:

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
and refused when its SHA-256 is not the one the job pins (ADR 0028). `--job-json <json>`
takes a job as a stored run holds it (the argument `job` of a check's record), with each of
its images by `--image <id>`, by identity in the store, once each in the job's order: what the
playground's Copy command writes for a check, so its requirements come with it. It is held as
a job file is: at most 1 MiB, no picture given inline, and a picture whose SHA-256 the job
pins must be one the store imported from a file of that SHA-256.

Nothing is measured before the judge looks, unless the run asks: the judge is offered `view`
and `align` and calls them, and a call of `align` is answered with its pictures and a fixed
sentence (`--details none`, the default, with `--projection visual_only`). This is the
control arm of the gate of details first, squint's baseline arm C, whole; the gate rejected
details first (`docs/results/m3-gates.md`, "The fourth run, the complete one"), so it is off
by default since plan m4, task A1. `--details first` turns it on, whole, as the gate's arm F
was: `align` registers the candidate onto the source, and the aligned picture, what `align`
found and every measurement open the conversation, each labelled with the tool and its mark
(ADR 0029); the judge may call `resize` beside `view` and `align`; and a call of `align` is
answered in `full`. `--projection` states the projection apart from either. `--detail`,
which names the tools measured first, is taken only with `--details first`, and refused
without it.

Two experimental options attack the verdicts that come back `unsure` (plan m3, task U1;
`docs/results/m3-unsure.md` measures them). `--details first --detail tiles`, for a job with
one image (`reality-checks`; `brief-compliance` with no reference), opens the conversation
with the candidate cut into four overlapping tiles at full resolution, each labelled with
its place (`docs/reference/tools/tiles.md`); a job with a pair is refused, naming its use
case. `--second-look on` gives the judge one more turn when its submission leaves a
mandatory item tentative, so that the verdict would be unsure: crops of what it cited for
those items (or the candidate in four tiles when it cited none), and the question to settle
them alone. It is inside the run's 16 turns and what the run reserved, and the verdict is
still worked out from the dispositions. `runs stats --unsure-by rule` counts what made the
unsure verdicts of a store unsure, by rule.

**Why a submission was rejected.** Every run of `check` under a use case keeps, however it
ends, each rejection of a submission by its class (`not_a_list`, `unknown_item`,
`missing_item`, `duplicate_item`, `bad_field`, `location`, `allowed_by`, `finding_cites`,
`evidence`, `not_alone`, `no_submission`) and why the run ended (`concluded`,
`invalid_submission`, `no_submission_after_repair`, `cut_off`, `ceiling`, `refused`,
`turns`, `fault`, `other`), in its diagnostic `rejections.json`. Classes and causes only:
the words the judge was sent are unchanged, and nothing it wrote is kept there. `ceiling` is
a turn cut off at its backend's output ceiling; `cut_off` a turn cut off again after its
limit was raised. `runs stats --rejections` counts both over a filter. A run answered from
the decision cache sent nothing and keeps none. The second look's own refusals are not
tallied: a second look that settles nothing leaves the first verdict, and `judged.json`
keeps its outcome and why. The diagnostic's contract is `rejections/2`.

**Submission v2** (`--submission v2`, experimental; ADR 0042). The judge is given its items
as an object with one key for each checklist item (the id with every character outside
letters, digits, `_`, `.` and `-` made `_`, each key's description naming its item),
near-misses that have one reading are read (a list sent
as text, a word in another case, an id in another case, an identical answer given twice,
evidence of a call that did not complete, one wrapper object) and each is recorded under
`recoveries` in `rejections.json`, and the one repair names every problem at once. v1, the
default, is unchanged. The format round of milestone E kept v1 as the default: v2 cut small
models' submission failures, but moved each small judge's accuracy or false-reject rate by
more than 2 points (`docs/results/e-submission.md`; ADR 0042).

**What one request may cost** is fixed by three settings of the backend, each held
before a byte is sent: `max_input_tokens` (40,000 when none is given),
`max_output_tokens` (4,096) and `max_pictures` (24), in a table
`[backends.judge.settings]` (`max_output_tokens` is at most 65,536 for Google). A
request is bound at its ceilings: for `claude-haiku-5-5`, 40,000 input tokens at 0.125
dollars a million and 4,096 output tokens at 0.50, which is 0.007048 dollars; a check
reserves that for each of its 16 turns, 0.112768 dollars, within the cap of 2.00. For
`gemini-3.8-flash` the bound is 40,000 at 0.75 and 4,096 at 3.75, 0.04536 dollars, 0.72576
for a check; for `gpt-6.1-terra`, 40,000 at 1.00 and 4,096 at 5.00, 0.06048 dollars,
0.96768 for a check; for the dearest rows, `gemini-4-argon` and `gpt-6.1-sol`, 1.93536.
A check asks 2,048 output tokens of each turn; a turn cut off there is asked again with
the ceiling, which every later turn of the check keeps. A turn cut off at the ceiling
ends the check as `judge_output_ceiling`, naming `max_output_tokens`.
Each turn is then settled at what its answer says it used. A conversation that needs
more raises the settings, and the cap with them (ADR 0026).

## What is refused

The file is read strictly, so that a slip is told and not guessed at. A refusal names
the key and never shows the value.

A key that is not known:

```toml refused
[backends.fill]
kind = "service"
modle = "bria-eraser"
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
