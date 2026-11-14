# 0011. The config file: one TOML file, named by a flag and read strictly

Status: accepted. Names ADR 0001 (the encoding its digest uses), ADR 0005 ("Redaction":
the secrets a reader is told of), ADR 0006 ("Resolution": what a capability's default
is) and ADR 0007 (`ConfigSource`, which answers this digest once task C4 builds a source
over the file)
Date: 2026-11-13
Amended: 2026-11-13, inside checkpoint 1B, with the second part of task C1. Added: "The
digest", with its consequences and the two alternatives it turned down. Changed: the
paragraph that said a config had no digest yet.
Amended: 2026-11-13, a second note, with task C2. Added: "The env file", which also
settles what ADR 0005 ("Redaction") left to the config: the quotes around a value, the
variable named in a refusal, and the least length. Changed: the sentence that said
reading a value was still to come.
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
`crates/whizbang-core/tests/it/config/` (`file`, `digest`, `env`, `examples`), and a test
named below is in that folder. Nothing is built from a config at this head: no command takes one, and
the roster, the cap, the policy and the settings of a run are made from it by tasks C4
and C5 of the plan.

This record was brought in two parts, because the task came to more than one pull
request's worth: first the file, its shape and how it is read; then "The digest". The
executor is still built with the digest of no bytes until task C4 gives it a source over
the file.

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
has no place for the value, which is read from an env file ("The env file", below).
Three rules keep a key out of the file and out of what is said about the file:

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

### The env file

The values are in one env file, which git ignores and nothing here writes. It is named
on the command line as the config is; that flag comes with the binary's wiring too. The
code is `config/env.rs`, and its tests are `env::`.

**The order.** The env file is read first, whole, by its own strict reader
(`EnvFile::read`, `EnvFile::parse`). Every value in it of 8 bytes or more is then a
secret the config's reader is told of (`EnvFile::secrets`), whether or not the config
names its variable; the config is read once with that set; and the variables the config
names are resolved from the same parsed file (`EnvFile::credentials`). So a value that
sits in the env file is never said back from a config, also when it was pasted under a
name nothing uses
(`env::the_env_file_is_read_first_and_a_config_that_holds_any_of_its_values_is_refused`).
A file that sets a variable to something that is not a secret, and is 8 bytes or longer,
makes a config that holds the same text unreadable; an env file is for keys.

**A line** is empty, a comment that starts with `#`, or `NAME=value`, with `export` and
a space before it if it likes. A name is a variable's name, as in the config. The value
loses the white space at its ends, a carriage return among it, and then one pair of
single or double quotes around it: ADR 0005 left the quotes to whoever reads the file, and here
they are taken off, so the bare key is what is handed over and what is taken out of
what is written
(`env::a_quoted_value_a_line_break_and_an_export_prefix_each_give_the_bare_key`). A
variable set to nothing is as one not set.

**What is refused,** each with the number of its line and nothing of the line, since a
line may be a key pasted where it does not belong: a line that is not an assignment; a
name given twice; a value that cannot be taken for one thing (a space outside quotes, a
quote that is not closed or not at the ends, a backslash inside double quotes, more than
4,096 bytes). And of the file, as of the config: what is not a file, and more than
64 KiB, each refused before anything is opened, with no more than the bound and one byte
read after; bytes that are not UTF-8, or an ASCII control character; more than 256
variables. No refusal has a path
(`env::a_line_that_is_not_an_assignment_and_a_name_given_twice_are_refused`,
`env::a_malformed_line_whose_name_holds_a_value_of_another_line_is_refused_without_showing_it`,
`env::a_file_over_its_bound_is_refused_from_its_length_and_no_refusal_has_a_path`).

**Credentials.** `EnvFile::credentials` answers, for a config, what each backend is
handed: the value of the variable its own `key_env` names and of the one its
`token_env` names, where the file sets them.

- **Only the backend whose table names a credential is handed it.** `Credentials::of`
  takes a capability and a backend's name. There is no question by a variable's name and
  none that answers every credential
  (`env::a_backend_is_handed_its_own_credential_and_cannot_ask_for_anothers`).
- **The executor's secrets** are every credential the config names that has a value
  (`Credentials::secrets`), and not the rest of the file.
- **A variable the file does not set** is listed by its name (`Credentials::missing`)
  and is no refusal: whether a backend can do without is the backend's, when one exists.
- **A value shorter than 8 bytes,** for a variable the config names, is refused, and
  then no credentials are made. The refusal names the variable and gives neither the
  value nor its length, which is less than ADR 0005's `SecretRefused` gives
  (`env::a_short_value_is_refused_with_the_variables_name_and_nothing_of_the_value`).
  A short value of a variable no backend names is not a credential and is not refused.
- **A name that holds a value.** A variable's name is said in a refusal and in the list
  of what is missing. A config read with the file's secrets cannot name a variable that
  holds one; for a config read without them, `credentials` refuses such a name and does
  not say it
  (`env::a_malformed_line_whose_name_holds_a_value_of_another_line_is_refused_without_showing_it`).
- **Nothing prints a value.** An `EnvFile` shows how many variables it holds, a
  `Credentials` its counts, what a backend is handed which of the two it has, and a
  `Credential` that it is one; the value is reached by `Credential::expose` alone. A
  marker key read this way is in no refusal, no `{:?}`, and no file of the store of a
  run that was given it as an argument and failed with it in its message
  (`env::a_marker_key_is_in_no_error_no_debug_no_record_and_no_journal_of_a_run_that_used_it`).

**What the env reader does not do,** each pinned by
`env::what_the_env_reader_does_not_do_is_known`: it expands nothing (`$NAME` in a value
is those letters) and reads no escape; a `#` with no space before it is part of the
value, and one after a space makes the value two things, which is refused; it does not
read the process's environment, so a variable set there and not in the file is missing;
and a value shorter than 8 bytes is a secret to nobody, neither looked for in a config
nor taken out of what is written.

### The digest

The digest of a config is of what the file says and not of its bytes: kind `config/1`,
hashed as ADR 0001 hashes every identity ("How an identity is hashed": the bytes
`whizbang:config/1` and a zero byte, then the fields; `u` is an unsigned integer as
eight bytes, big-endian; `b` is a length as eight bytes, big-endian, and that many
bytes). A text is its UTF-8 bytes. "Sorted" is by the bytes of a name. A field that may
be absent is `u` 0, or `u` 1 and then the field. The fields, in order:

1. `u`, how many capabilities are given at least one backend. Then for each of those,
   sorted by its name (`fill`, `judge`, `segment`):
   1. `b`, the capability's name;
   2. the default's name, `b`, which may be absent;
   3. `u`, how many backends. Then for each, sorted by its name:
      1. `b`, its name; `b`, its kind;
      2. its model, its provider, its address, the name of its key's variable and the
         name of its token's variable, in that order, each `b` and each of which may be
         absent;
      3. `u`, how many settings. Then for each, sorted by its key: `b`, the key; then
         `u` 1 and `u` the whole number, as the 64 bits of its two's complement; or `u`
         2 and `u` 0 for false or 1 for true; or `u` 3 and `b` the text.
2. The cap of one call: `u` millionths of a dollar, which may be absent.
3. `u`, how many recipients. Then for each, sorted by its name:
   1. `b`, its name; `b`, its origin; its account, `b`, which may be absent;
   2. `u`, how many upstreams, and each as `b`, sorted;
   3. `u`, how many classes, and each as `b` its name, sorted;
   4. `u` 1 if it may be probed, else 0.

The short form enters as what it is: a backend named `default`, with the default
`default`. The empty config hashes 45 bytes: the 17 of `whizbang:config/1`, a zero byte,
and three fields of `u` 0, nine bytes each. Three digests are fixed
(`digest::the_digests_of_three_fixed_configs_are_the_records`; they were worked out from
this section alone with a few lines of a scripting language's standard library, not
from the code):

| Config | Digest |
|---|---|
| The empty one | `sha256:9d60a543ea39be1ca1bc2c4e1a90094a59ded6bdcfdf1c21436f4026f207c4a6` |
| The example of `SPEC.md` section 4 | `sha256:e0f7ca76571f20c077084f4dcb18a57c0adf857b37d59928546508937fa31c7f` |
| The config `FULL` of the digest's tests, with something in every place | `sha256:c151bf69e75bfec361a2018365535e49731847d0dbf6d13f4bb71b06356d6399` |

What follows from hashing what was read:

- **Two files that say the same have one digest:** comments, spacing, line endings, the
  order of tables and of keys, the order of a list of classes or upstreams, and every
  way TOML has of writing one value (the kinds of string, an escape, an inline table,
  dotted keys, the ways of writing a number). A cap of `2`, `2.0` and `2.50` are one
  amount (`digest::two_files_that_differ_in_comments_spacing_and_order_have_one_digest`,
  `digest::the_ways_toml_writes_one_value_have_one_digest`).
- **A table with nothing in it is as no table,** and saying what is so unless said
  otherwise (`probe = false`, `upstreams = []`) is as not saying it
  (`digest::a_table_with_nothing_in_it_is_as_no_table`).
- **A change to any setting changes it:** every key of every table is in the fields
  above, with its place marked by a count or a length, so no two configs run together
  to the same bytes (`digest::a_change_to_any_setting_changes_the_digest`,
  `digest::where_one_text_ends_and_the_next_begins_is_in_the_digest`).
- **No secret is in it.** A variable is in the digest by its name. Its value is in no
  config, so a key that is rotated under the same variable changes no digest; what a
  changed key does to a call is task C5's
  (`digest::a_variable_is_in_the_digest_by_its_name_and_a_value_by_nothing`).

A key added to the file later that is absent from older files must hash as absent, or it
is a new kind, `config/2`, and every digest changes with it.

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
  `https://h.example:443` are two texts, two configs and two digests. A host in capitals is refused,
  so that case is not a third way.

## Consequences

- Tasks C4 and C5 build the roster, the cap, the policy and a run's settings from one
  value that cannot be made unchecked.
- The spec's example stands as written; `SPEC.md` is not changed.
- A config cannot carry a credential by any documented way. A person who wants a key in
  a file puts it in the env file.
- Keys come from the env file alone, never from the environment of the process: what a
  run could use does not depend on the shell it was started from.
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
- Reversing the digest's form, or the meaning of a key, changes every digest: each
  prepared call goes stale once, and records written before keep the old digest. Adding
  an optional key does not.
- Not ported from the old tools, each with its line under "Not ported" in
  `docs/carry-over.md`: `${VAR}` expansion, `--set` overrides, a default place for the
  file, a version key, and the JSON schema of the config. So is rascal's identity of a config,
  hashed from canonical JSON.

## Alternatives

- **The digest of the file's bytes.** A comment or a reordered table would make every
  prepared call stale, and two machines with the same settings would record two
  digests.
- **Hash a canonical JSON of the config,** as rascal did. Turned down for every identity
  in ADR 0001: the bytes depend on how a library orders keys and prints numbers.
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
