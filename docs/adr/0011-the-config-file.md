# 0011. The config file: one TOML file, named by a flag and read strictly

Status: accepted. Names ADR 0005 ("Redaction": the secrets a reader is told of) and ADR
0006 ("Resolution": what a capability's default is)
Date: 2026-11-13
Spec: section 4, "Backends" (config, resolution, credentials); section 6, "Spend";
section 10, "Egress"

## Context

`SPEC.md` section 4 says every backend shares the policy around a call: "one shape for
choosing a backend per capability", a resolution that is "an explicit per-call choice,
then the config default", and credentials "named in config and given only to the backend
that needs them". It gives one example, two tables under `backends`. Section 10 says
"Config lists which classes of data each recipient may receive", and the plan of
checkpoint 1B puts a cap on one call in the config and the allowance in the ledger.

A run record names "the digest of the config" its settings came from (ADR 0005), a
prepared call's fingerprint holds that digest, and a call whose config changed between
its preparation and its admission is refused as stale (ADR 0007). Until now there was no
config: the executor was built with the digest of no bytes.

The two old tools each read a TOML file. Squint's (`crates/squint-core/src/config.rs`,
its ADR 0004) expanded `${VAR}` from the environment into the config, so a key's value
was inside the config type and was taken out again wherever the config was written.
Rascal's (`crates/rascal-core/src/config.rs`) named the variable (`key_env`) and hashed a
canonical JSON of the config as its identity. Both read with `serde` and
`deny_unknown_fields`, whose refusals quote the value that was wrong.

A config is a file a person writes. It is untrusted in the plain sense: a mistake in it
must be refused with a sentence that can be printed, and the commonest serious mistake is
a key pasted where it does not belong.

## Decision

The code is `crates/whizbang-core/src/config/`; its tests are
`crates/whizbang-core/tests/it/config/` (`file`, `examples`), and a test named below is
in that folder. Nothing is built from a config at this head: no command takes one, and
the roster, the cap, the policy and the settings of a run are made from it by tasks C4
and C5 of the plan.

This record is brought in two parts, because the task came to more than one pull
request's worth. This is the first: the file, its shape and how it is read. **The digest
of a config is the second part,** a section this record gains with the code and the
fixed digests that hold it; at this head a config has no digest, and the executor is
still built with the digest of no bytes.

### Where the file is

One TOML file, named on the command line by `--config <file>`, as the store is named by
`--store`. There is no default place: no file in the working folder, none in a user's
folder, and no variable that names one. A command given no config has the empty config:
no backend, no cap, no recipient (`Config::empty`). The flag itself comes with the task
that wires the binary; the reader takes a path (`Config::read`) or bytes
(`Config::parse`).

### What the file holds

Five tables may stand at the top, and nothing else.

**`backends`** holds one table for each capability that is given a backend: `segment`,
`fill`, `judge`. A capability's table is written in one of two forms.

The short form is one backend. It is the form of the spec's example, which is read as it
stands (`file::the_specs_example_is_read_to_the_backends_it_shows`):

```toml
[backends.fill]
kind = "service"
provider = "fal"
model = "bria-erase"
key_env = "FAL_KEY"
```

A table that holds the key `kind` is the short form. Its backend is named `default` and
is the capability's default, the backend that answers a call that names none (ADR 0006,
"Resolution").

The long form is several backends, each in a table under the name a call chooses it by,
and perhaps the key `default`, whose text is the name of one of them:

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
```

A name keeps the rule of an action's name (ADR 0003). With no `default` the capability
has none, however many backends it has, as ADR 0006 has it. A `default` that names no
backend written there is refused. In the long form no backend is named `default` or
`kind`, since those two keys are taken; so the two forms never say the same thing, and a
file cannot mix them. Nor is one named as a backend's other keys are (`model`,
`provider`, `url`, `key_env`, `token_env`, `settings`): a table that holds one of those
and no `kind` is one backend written without its kind, and is told so
(`file::several_backends_of_one_capability_are_written_by_name_with_a_default`).

A backend has these keys and no others:

| Key | Is | Rule |
|---|---|---|
| `kind` | Which code answers. Required | A name |
| `model` | The model it is asked for | A plain text of 128 characters at most |
| `provider` | The service it is reached through | A name |
| `url` | Its address | `http://` or `https://`, a host, perhaps a port and a path; 2,048 characters at most |
| `key_env`, `token_env` | The variable its key, or its token, is in | A variable's name |
| `settings` | A table of the kind's own settings | 32 at most; a key is a name without `-`; a value is a plain text of 1,024 characters at most, a whole number, or true or false |

What a kind makes of `model`, `provider`, `url` and `settings`, and which it requires,
is the kind's, when a kind exists (task C4). A setting's value is never a number with a
fraction, a date, a list or a table: an amount is written in whole units or as a text,
so that a setting has one way to be hashed.

A plain text is printable ASCII (the bytes 0x20 to 0x7E), not empty, with no space at
either end. A host is lower-case letters, digits and hyphens in parts with dots between
them, 253 characters at most; a port is 1 to 65535 with no leading zero. A variable's
name is capital letters, digits and `_`, not a digit first, 64 characters at most
(`file::a_text_is_held_to_its_rule_and_its_length`,
`file::an_address_that_could_hold_a_credential_is_refused_and_not_shown`).

**`spend`** holds one key, `call_cap_usd`: the most one call may reserve, in US dollars,
more than 0 and at most 1,000,000. It is a whole number or a number with at most six
decimal places, and is kept in millionths of a dollar, the unit of an estimate (ADR
0007). A number with a fraction is taken times 1,000,000 and rounded to the nearest
whole number; that number divided by 1,000,000 must be the number that was written, or
the amount is refused, so nothing finer than a millionth is rounded away. Multiplying,
rounding and dividing doubles gives the same answer on every machine. The allowance is
not in the config: it is the ledger's, set by the owner's command
(`file::the_cap_of_one_call_is_an_amount_in_dollars_read_to_millionths`).

**`egress`** holds one table, `recipients`, of at most 32 recipients by name:

```toml
[egress.recipients.fal]
origin = "https://fal.example"
account = "team-1234"
upstreams = ["https://storage.fal.example"]
classes = ["prompts", "source_images"]
probe = true
```

| Key | Is | Rule |
|---|---|---|
| `origin` | Where data is sent. Required | A scheme, a host and perhaps a port, and nothing else |
| `account` | The account used with it | A plain text of 128 characters at most |
| `upstreams` | The origins it may forward to | 16 at most, each an origin; none twice, and not its own |
| `classes` | The classes of data it may receive. Required, and may be `[]` | Of `source_images`, `derived_images`, `prompts`, `evidence`; none twice |
| `probe` | Whether it may be probed | True or false; false when not written |

A config that names no recipient approves none, and one that names a recipient with
`classes = []` approves it for nothing. The config says what is approved; what a
recipient is, how one is matched to a backend and what the gate does with this table are
the egress tasks' (B2, E2), and this record decides only how the table is written
(`file::a_recipient_is_an_origin_with_the_classes_of_data_it_may_receive`).

**`models`** is a table of the file and holds no key yet: any key in it is refused as not
known. It is named now so that its heading can be written, and it is given its keys by
an amendment to this record when a task needs one. **`tools`** is refused, with or
without anything in it: its shape is the first decision of milestone 2
(`file::tools_is_refused_until_it_has_a_shape_and_models_holds_nothing_yet`).

Each list has its bound: 16 backends for a capability, 32 settings for a backend, 32
recipients, 16 upstreams (`file::more_than_the_bound_of_a_list_is_refused`).

### A credential is named, never held

A backend says which variable its credential is in. The config type holds that name and
has no place for the value; reading the value from an env file is task C2's. Three
rules keep a key out of the file and out of what is said about the file:

- **What stands where a variable's name belongs is never shown.** If it is not a
  variable's name it is refused, and the refusal names the key and says that what was
  written is not shown; the same when it is not a text at all
  (`file::a_value_where_a_variables_name_belongs_is_refused_without_being_shown`).
- **An address has no place for a credential.** A `url` with a user, a password, a query
  or a fragment is refused, and so is an origin with anything after its port
  (`file::an_address_that_could_hold_a_credential_is_refused_and_not_shown`).
- **A file that holds a secret the reader was told of is refused whole.** The reader is
  given the secrets (`Secrets`, ADR 0005) that must never be written, as the values of
  an env file are. It looks for them twice: over the file's bytes as they are, before
  the bytes are taken for text or parsed, so a comment and a line that is not TOML are
  looked at like any other; and over every key and every text as the format decoded it,
  since an escape can spell what the bytes do not hold. A file that holds one, as a
  backend's name, a setting's key, a value or a comment, is refused as `HoldsSecret`,
  and that refusal shows nothing of the file. Both looks come before any key is escaped,
  cut or shown, so no cut can fall inside a secret.
- **A refusal's whole path is looked at too.** A refusal names a key by its path from
  the top, with dots, and a secret can stand across two keys of that path, or across a
  key and a word the reader adds (`.kind`, `.origin`), where no key alone holds it and
  the two halves are on different lines of the file. So the path is looked at whole, as
  it was read and before it is escaped or cut, in the one place a refusal that names a
  key is made (`At::refuse`); a path that holds a secret is `HoldsSecret`. A key given
  twice is named by its last part alone ("What a refusal says"), which is looked at
  whole in the same way
  (`file::a_name_or_a_key_that_holds_a_secret_is_refused_whole_and_not_shown`, for all
  three looks).

### How the file is read

- **What it is, and its length, first.** Both are asked of the file system before
  anything is opened. What is not a file (a folder, a pipe, a device, a terminal) is
  refused, so nothing is opened that could block or never end; and a file over 256 KiB
  (`CONFIG_BYTES_LIMIT`) is refused from its length
  (`file::what_is_not_a_file_is_refused_before_it_is_opened`,
  `file::a_file_over_the_bound_is_refused_from_its_length`,
  `file::a_file_over_the_bound_is_refused_before_it_is_opened_to_read`). No more than
  the bound and one byte is then asked of what was opened, for a file that grew between
  the two (`Config::read_from`, which a file is read through;
  `file::no_more_than_the_bound_and_one_byte_is_asked_of_what_is_read`).
- **Text.** The bytes are UTF-8, with no ASCII control character but a tab, a line feed
  and a carriage return, anywhere, a comment included
  (`file::bytes_that_are_not_text_and_control_characters_are_refused`).
- **TOML, by the `toml` crate,** which refuses a key or a table given twice and stops at
  a depth of nesting (`file::a_key_given_twice_is_refused_naming_the_key`,
  `file::text_that_is_not_toml_is_refused_with_its_line_and_none_of_its_words`).
- **Then by hand.** The tables are walked by code written here, not by a derived
  reader: every key is looked up in a closed list, and every value is held to its kind
  and its rule (`file::an_unknown_table_or_key_is_refused_naming_the_key`,
  `file::a_value_of_the_wrong_kind_is_refused_naming_the_key_and_never_the_value`).

A `Config` can be made by reading or as the empty one, and in no other way: its fields
and those of a backend and a recipient are private, and none of the three can be read
from TOML or JSON by other code. That holds by the absence of an implementation, which
no test at run time can show. A config's `{:?}` shows how many backends and recipients
it has and whether it has a cap; a backend's and a recipient's show which parts are
there; none shows a value, since a text a person wrote into a setting is not
known to be harmless (`file::a_config_prints_its_shape_and_none_of_its_values`).

### What a refusal says

A refusal is one of eight (`ConfigRefused`): the file could not be read, with the kind
of failure; what the path names is not a file; it is too large; it is not text, with the line; it holds a secret; it is not
TOML, with the line; a key is given twice, with the key and the line; or a key is not
known, is missing, or has a value that does not belong, with the key from the top of the
file and the reason.

- **No path.** The caller knows which file it asked for (`file::a_refusal_shows_no_path`).
- **No value.** Every sentence is written in the reader: it says what kind of thing
  belongs at a key and what kind was found, and never quotes what was found. The
  sentence of the `toml` crate is not passed on, since it is not written here.
- **A key given twice is named by its last part:** the one key the format's reader
  points at where it is given the second time, without the table it is in and without
  the keys before it in a dotted key. The
  format's reader says where the second key stands and not which table holds it, and
  nothing is taken from another line of the file to make up the rest: a line above that
  looks like a heading may be the text of a value
  (`file::a_key_given_twice_is_refused_naming_the_key`).
- **A key, escaped and cut.** A key that came from the file is shown with each character
  escaped (`char::escape_default`), so that what is shown is one line of ASCII, and its
  first 64 characters of that, with three dots when there were more
  (`KEY_SHOWN_LIMIT`; `file::a_key_in_a_refusal_is_escaped_and_cut`).

### Which settings a run records

`asked.settings` of a run record (ADR 0005) is decided here and filled by task C5; no
code at this head fills it. A run records, as names and values sorted by name:

- for each backend its call resolved, `backends.<capability>.<name>.kind`, and `.model`,
  `.provider` and `.url` where the file gives them, and each of its settings as
  `backends.<capability>.<name>.settings.<key>`;
- `spend.call_cap_micros`, the cap in millionths, when the file gives one.

It does not record the name of a variable, any credential, a backend the call did not
resolve, or the `egress` table: where data went is the record's `egress` group, and the
config's digest in `asked.config` stands for the rest.

### What the reader does not do

`file::what_the_reader_does_not_find_or_tell_apart_is_known` pins each of these as so.

- **It does not know a key by its shape.** A key the reader was not told of, written as
  a model or into a setting's text, is read. Where a variable's name belongs it is
  refused only because it is no variable's name: a key made of capital letters and
  digits alone is read as a name. And pasted as a KEY of the file, where no key is
  known, it is said back in the refusal, its first 64 characters: a refusal shows keys.
- **It looks for a secret in the bytes, the keys and the texts, and not in a number.**
  A secret of digits written as a number with a mark between its digits (`1234_5678`)
  is in none of the three, and is read as that number.
- **A key given twice is shown without its table.** A secret that stands across the
  table's name and the key is not whole in what is shown, and the key, its last part,
  is shown.
- **An amount is a double before it is millionths.** Digits past what a double holds
  are not seen: `2.5000000000000000001` is 2.5, and is not refused as finer than a
  millionth.
- **It finds a secret only in the forms redaction searches for** (ADR 0005, "What
  redaction does not find"), over the bytes and over the decoded keys and texts. One in
  another form, as with its letters in the other case, is not found.
- **It does not tell two origins apart by what they reach.** `https://h.example` and
  `https://h.example:443` are two texts and two configs that are not equal. A host in capitals is refused,
  so that case is not a third way.

## Consequences

- Tasks C4 and C5 build the roster, the cap, the policy and a run's settings from one
  value that cannot be made unchecked.
- The spec's example stands as written; `SPEC.md` is not changed.
- A config cannot carry a credential by any documented way. A person who wants a key in
  a file puts it in the env file (task C2).
- Text in a config is ASCII. A model or an account with a letter outside it cannot be
  written; that is a limit, taken so that every sentence and every record that quotes a
  config is one line of ASCII.
- The sentence of a refusal names a key and a line only for a key given twice or text
  that is not TOML; a key that is not known is named without its line, because the
  tables are walked after the format's reader has dropped where each key stood.
- Whether a key was given twice is told from the words of the `toml` crate's error, and
  the key from where the error points. A version of the crate that words it otherwise
  would turn that refusal into "not TOML" with its line, and
  `file::a_key_given_twice_is_refused_naming_the_key` would fail.
- Not ported from the old tools, each with its line under "Not ported" in
  `docs/carry-over.md`: `${VAR}` expansion, `--set` overrides, a default place for the
  file, a version key, and the JSON schema of the config. Rascal's identity of a config
  is the second part's.

## Alternatives

- **Expand `${VAR}` into the config,** as squint did. The value would be inside the
  type that is hashed, printed and recorded, and every one of those would have to take
  it out again.
- **A derived reader with unknown fields denied.** Its refusals quote the value that was
  wrong, and a value is where a pasted key is.
- **Look for the file in the working folder and a user's folder.** Which config a run
  used would depend on where it was started; the store's location was settled the same
  way at checkpoint 1A.
- **A version key in the file.** The reader refuses every key it does not know, so a
  file written for a later shape is already refused, with the key named.
- **Numbers with a fraction as settings.** A double has several written forms, and a
  setting is to have one way to be hashed and recorded; nothing asks for one yet.
- **One form for a capability's backends.** The long form alone would make the spec's
  example wrong; the short form alone cannot name two.
