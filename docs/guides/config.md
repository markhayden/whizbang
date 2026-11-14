# The config file

One TOML file says which backend answers each capability, what one call may spend, and
who may receive data. This page is how to write it. Why it is this way, and exactly how
its digest is made, is in `docs/adr/0011-the-config-file.md`.

Every example on this page is read by the fast tier: a block that is shown as read is
read, and a block that is shown as refused is refused with the sentence under it
(`config::examples::every_example_in_the_guide_and_in_the_record_is_read`,
`config::examples::the_guides_env_file_is_read_with_its_config`).

Nothing uses a config yet. The readers of the config and of the env file are built
(checkpoint 1B, tasks C1 and C2); the commands that take them, and everything a config
switches on, come with the tasks after. There is no default place for the file: it is
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
| `settings` | The kind's own settings: texts, whole numbers, and true or false |

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
the quotes is refused, and so are quotes around nothing. A `#` straight after a value,
with no space before it, is part of the value; a comment has a line of its own. Save the
file as plain UTF-8 with no byte-order mark, which some editors add and which is refused.
A link to a file is followed, and what it leads to is read. Nothing is expanded, and the
environment of your shell is not read: a key is in this file or it is not set. Only the variables your config names are used, each by
the backend that names it. A key shorter than 8 bytes is refused, by the name of its
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
table `tools`, which has no shape until milestone 2; any key in the table `models`,
which has none yet; and a text with a letter outside ASCII, outside a comment.

## Two files that say the same

A config has a digest, which a run records. It is of what the file says, not of its
bytes: comments, spacing, the order of tables and keys, and the ways TOML has of writing
one value change nothing. Any change to a setting changes it.
