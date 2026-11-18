# 0011. The config file: one TOML file, named by a flag and read strictly

Status: accepted. Names ADR 0001 (the encoding its digest uses), ADR 0005 ("Redaction":
the secrets a reader is told of), ADR 0006 ("Resolution": what a capability's default
is), ADR 0007 and ADR 0015 (`ConfigSource`, which the source over both files is) and
ADR 0012 (an origin, an address, an account, a recipient and the
classes of data, which the reader makes by that record's rules)
Date: 2026-11-13
Amended: 2026-11-13, inside checkpoint 1B, with the second part of task C1. Added: "The
digest", with its consequences and the two alternatives it turned down. Changed: the
paragraph that said a config had no digest yet.
Amended: 2026-11-14, a second note, after ADR 0012 gave the egress types. Changed: an
origin, a backend's address, an account, a recipient and its classes are made by those
types and held to their rules, where this record had rules of its own for each; one
recipient is approved once; the two rules that stay the file's own are marked as such.
And in "The digest": an origin and a backend's address are hashed as an origin writes
itself, so the digest of a config with an origin in capitals, or with its scheme's own
port written, is not what it was for one day; no digest had been stored, since nothing
makes one of a config yet.
Amended: 2026-11-14, a third note, with task C2. Added: "The env file", which also
settles what ADR 0005 ("Redaction") left to the config: the quotes around a value, the
variable named in a refusal, and the least length. Changed: the sentence that said
reading a value was still to come.
Amended: 2026-11-14, a fifth note, with ADR 0014. Changed in "The env file": the forms
in which a value of the env file is found in a config are those of ADR 0014.
Amended: 2026-11-14, a fourth note, after the review of task C2. Changed in "The env
file": white space at an end inside quotes, and quotes around nothing, are refused; no
set of secrets is made by leaving a refusal out; `Loaded::read` is the way in.
Amended: 2026-11-14, a sixth note, with the part of tasks C4 and C5 that needs no
snapshot of the config. Added: "Kinds", "What a config offers" and "Both files, each
time"; and under "Which settings a run records", the function that makes them and what
it refuses. Changed in "The env file": a variable the file does not set makes the
backend that names it not ready, where this record had left that to the backend; and
the paragraphs that said nothing was made of a config.
Amended: 2026-11-14, a seventh note, with task I6 (ADR 0015, "One snapshot of the
config"). Changed in "Kinds": a kind makes a backend (`Kind::make`), where it declared
one, and its sentence is cleaned and cut where it is kept. In "What a config offers":
a kind that makes a backend of another capability; what is offered holds the backends
that were made; two backends that declare one model are settled; and a key read as a
variable's name. In "Both files, each time": what a config read with no secrets
protects, the order of the refusals, and what still waits. Under "Which settings a run
records": a host written in capitals; a config is a snapshot's `Settings`.
Amended: 2026-11-14, an eighth note, with the rest of task C4. Added: "The source over
both files". Decided in "The digest": it covers nothing of the env file, and why.
Changed: `Loaded::of_files` and `Offers::of` each have a second form for a reader that
kept the credentials of earlier readings; and the paragraphs that said no source read
a config for an executor.
Amended: 2026-11-14, a ninth note, with task C5. Added in "The source over both files":
the cap, the policy and the settings in the snapshot, and what an upstream that holds
a secret meets. Changed in "What the file holds" and "Which settings a run records": a
backend's name and a setting's key are held together to the name a record takes, where
this record let a config be read whose runs could not record their settings.
Amended: 2026-11-14, a tenth note, after the review of tasks C4 and C5. Added to the
limits of "The source over both files": what reading the two files one after the other
can pair. Changed in "The digest": what a guess at a key would be hashed with. And of
`Files::knowing`: the secrets a source is told of from the start are kept however many
they are.
Amended: 2026-11-15, an eleventh note, with task S3. Changed in "The source over both
files": the cap's clause is shown with the part that holds a call to the cap, where a
part of the test's own stood for it.
Amended: 2026-11-15, a twelfth note, with task W1 (ADR 0015, "The binary wired"). The
two flags exist; the binary's executor is built over the source, with the same
secrets given to both; and under "The source over both files", "Limits": reading the
env file a second time was considered and is not done, with why. And under "What a
config offers": over an inventory.
Spec: section 4, "Backends" (config, resolution, credentials); section 6, "Spend";
section 10, "Egress"
Amended: 2026-11-15, at the exit check of checkpoint 1B (task Z1): notes after sentences
that said something was still to come; and under "Kinds", that a backend which names a
recipient must not declare a version written as a bare identity, which ADR 0015
already decided. No decision changed.
Amended: 2026-11-15, a thirteenth note, with task N3a of checkpoint 1C
(`docs/plans/m1c-local-pack.md`). The table `models` is given its two keys: `dir`,
where the models directory is, and `download_least_kib_per_second`, the least rate
of a download ("What the file holds"). Added to "The digest": the table is one more
field, hashed only when it gives a key, so no digest of a file without one moved;
and a fourth fixed digest. Under "Which settings a run records": neither key is
recorded, and why.
Amended: 2026-11-16, a fourteenth note, with task N3b of checkpoint 1C. Changed in "The
digest": the `models` table is in no digest, where the thirteenth note had it as a
fourth field hashed when it gave a key. Where the models directory is, and how slow a
download may be, are not part of what a run means, and with the table in the digest a
config that only moved the directory made every preparation made before it stale.
The three digests fixed before the thirteenth note did not move, and are held again
by the test that replaces the fourth's; the fourth, of a `models` table alone, is now
the empty config's. No digest of a config with a `models` table had been kept by
anyone but a test: the binary built with the table in the digest was on `main` for
one day, with no model it could install. Changed in "What the file holds":
`download_least_kib_per_second` is given to the download client by `whizbang models
get`.
Amended: 2026-11-16, a fifteenth note, with task N4 of checkpoint 1C (D12). Added: "A
download's token": the `models` table's third key, `tokens`, which names for an origin
where its token is, the Hub's sign-in or a variable of the env file; where the binary
reads the sign-in, which of the Hub's variables it follows and why; what it takes from
the file, and what it refuses; and that a variable a token is named in is one the
config names, read from the env file as a backend's key is.
Amended: 2026-11-17, a sixteenth note, with task P6a of checkpoint 1C. Added: "The kind
`pack`": the first kind the binary registers, its table and its settings, the model's
and its own, each with its rule and the words of its refusal. Changed in "Kinds": the
sentence that said the shipped binary registers none.

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
`crates/whizbang-core/tests/it/config/` (`file`, `digest`, `env`, `examples`, `roster`,
`files`, `settings`, `source`), and a test named below is in that folder. No command
takes a config at this head. What is made from a config is what a snapshot of it holds
(ADR 0015, "One snapshot of the config"): what it offers ("What a config offers"), with
the backends made, and the settings a run records; both files are read again on each
ask ("Both files, each time"); and the source an executor asks makes a snapshot of them
("The source over both files"), with the cap, the policy and the settings the config
gives.

This record was brought in two parts, because the task came to more than one pull
request's worth: first the file, its shape and how it is read; then "The digest". The
binary's executor was built with the digest of no bytes until task W1 gave it the
source over the files (ADR 0015, "The binary wired").

### Where the file is

One TOML file, named on the command line by `--config <file>`, as the store is named by
`--store`. There is no default place: no file in the working folder, none in a user's
folder, and no variable that names one. A command given no config has the empty config:
no backend, no cap, no recipient (`Config::empty`). The flag came with the task that
wired the binary (`whizbang rascal grow --config <file>`; ADR 0015, "The binary
wired"), and a run under no config records the empty config's digest; the reader takes
a path (`Config::read`) or bytes (`Config::parse`).

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
| `url` | Its address | An address as ADR 0012 has one, and, the file's own rule, with no query |
| `key_env`, `token_env` | The variable its key, or its token, is in | A variable's name |
| `settings` | A table of the kind's own settings | 32 at most; a key is a name without `-`; a value is a plain text of 1,024 characters at most, a whole number, or true or false; and the name the setting is recorded under, `backends.<capability>.<name>.settings.<key>`, is 128 characters at most |

**A backend's name and a setting's key are held together to what a record takes.** A
run records a setting under `backends.<capability>.<name>.settings.<key>` ("Which
settings a run records"), and a record takes a name of 128 characters
(`SETTING_NAME_LIMIT`, ADR 0013). A name and a key of 64 each would come to 152 for
`fill`, 153 for `judge` and 155 for `segment`. So a setting whose recorded name is
longer than the record's bound is refused where the config is read, naming the key and
saying by how many characters the two are too long: a name and a key may be 104
together for `fill`, 103 for `judge` and 101 for `segment`. The reader and the function
that makes a run's settings make the name with one function and hold it to the
record's one constant, so the two cannot part; the short form's backend is named
`default`, which every key fits
(`settings::the_longest_names_a_config_can_hold_are_recorded_and_one_character_more_is_refused_where_the_config_is_read`).
The record's bound stands because it is the record's: `run/3` states it in its schema
and its ADR, a record is read by other tools than this one, and a name that long is
two names of 64 characters, which nothing needs.

What a kind makes of `model`, `provider`, `url` and `settings`, and which it requires,
is the kind's, when a kind exists (task C4). A setting's value is never a number with a
fraction, a date, a list or a table: an amount is written in whole units or as a text,
so that a setting has one way to be hashed.

A plain text is printable ASCII (the bytes 0x20 to 0x7E), not empty, with no space at
either end. A variable's name is capital letters, digits and `_`, not a digit first, 64
characters at most (`file::a_text_is_held_to_its_rule_and_its_length`).

An address and an origin are what ADR 0012 says they are, made by its constructors
(`Address::of`, `Origin::of`) with the secrets the reader was told of, and this record
does not say their rules again: the scheme, the host and its labels, a host written in
numbers, the port, the length, that an `@` and a fragment are refused anywhere, and
that a secret is looked for in the text as given and again in the origin as it is
kept. An origin is kept as that type writes it, the scheme and the host in lower case
and the port left out when it is the scheme's own, so `https://h.example`,
`https://h.example:443` and `HTTPS://H.Example` are one origin and one config. Two
rules are the file's own, on top of those:

- **A backend's address has no query,** though an address may have one: a query is
  where a key is written.
- **An origin has nothing after its port,** though `Origin::of` would take an address
  and drop its path: what a person wrote there is not dropped without a word.

(`file::an_address_that_could_hold_a_credential_is_refused_and_not_shown`,
`file::a_recipient_is_an_origin_with_the_classes_of_data_it_may_receive`).

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
| `origin` | Where data is sent. Required | An origin, with nothing after its port |
| `account` | The account used with it | An account as ADR 0012 has one: 128 characters at most, with no space, no `"` and no `\` |
| `upstreams` | The origins it may forward to | 16 at most, each an origin; none twice, and not its own, compared as origins and not as texts |
| `classes` | The classes of data it may receive. Required, and may be `[]` | Of `source_images`, `derived_images`, `prompts`, `evidence`; none twice |
| `probe` | Whether it may be probed | True or false; false when not written |

A config that names no recipient approves none, and one that names a recipient with
`classes = []` approves it for nothing. What the reader makes of an entry is an
`Approval`: a `Recipient` of ADR 0012 (its origin, its account, its upstreams, with
that type's rules for how many upstreams there are and that none is given twice or is
the recipient's own), the `Classes` it may receive, and whether it may be probed.

**One recipient is approved once.** Two entries with one origin and one account are
refused, by the `origin` of the second: under two names a recipient could be approved
for two sets of classes, and which held would be the reader's guess. The same origin
under another account, or with an account beside one with none, is another recipient.
How an approval is matched to a backend and what the gate does with it are the egress
tasks' (`file::a_recipient_is_an_origin_with_the_classes_of_data_it_may_receive`).

**`models`** holds two keys, each of which may be left out, and no other (task N3a
of checkpoint 1C gave it them; until then any key in it was refused):

```toml
[models]
dir = "/srv/whizbang/models"
download_least_kib_per_second = 256
```

| Key | Is | Rule |
|---|---|---|
| `dir` | Where the models directory is (ADR 0017, "Where the directory is") | An absolute path, as it is written: it starts with `/`, holds no control character, and is 1,024 bytes at most (`MODELS_DIR_LIMIT`). It may hold spaces and letters outside ASCII, as a folder's name may |
| `download_least_kib_per_second` | The least rate of a model's download, in kibibytes a second | A whole number from 1 to 1,048,576 (`DOWNLOAD_LEAST_KIB_LIMIT`), which is a gibibyte a second |

- **`dir` is the second of three that may say where the directory is:** a command's
  flag is asked first, and the platform's data folder serves when neither says.
  ADR 0017 has the order and what is done with the folder; the reader only keeps
  the text (`Config::models_dir`). Nothing is asked of the disk when the file is
  read: a path that is not there, or is another user's, is said by the opening of
  the directory. **It is absolute** because a config is not tied to a working
  folder: a relative path would be another folder from each place a command is run
  in. A `~` is not read as anybody's home folder. No refusal shows what was
  written.
- **`download_least_kib_per_second` is what a download must keep up over any whole
  minute** or be ended as timed out (plan 1C, "Rules": the download client's
  limits). Whoever makes the download client gives it this, or 256 when the file
  gives none (`DOWNLOAD_LEAST_KIB_PER_SECOND`): `whizbang models get` makes the
  client with it (task N3b;
  `models::get_makes_the_download_client_with_the_configs_least_rate_or_the_default`,
  and the client's own test of a slow download, task N1's). A whole
  number of kibibytes, since a setting is never a number with a fraction; one at
  least, since a least rate of nothing is no limit, and a download that never ends
  is what the limit is for.

(`file::the_models_table_names_the_directory_and_the_least_rate_of_a_download`.)
**`tools`** is refused, with or without anything in it: its shape is the first
decision of milestone 2
(`file::tools_is_refused_until_it_has_a_shape_and_a_table_with_nothing_in_it_is_as_no_table`).

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

### A download's token

Task N4 of checkpoint 1C, for plan 1C's D12, which the owner chose on 2026-11-15 "for
now": a model's files may be behind a sign-in, and whizbang reads the token of the
sign-in the Hub's command-line tool keeps on the machine, or a key of the env file. The
config says which, for each origin, in the `models` table:

```toml
[[models.tokens]]
origin = "https://huggingface.co"
sign_in = "huggingface"

[[models.tokens]]
origin = "https://weights.example.org"
key_env = "WEIGHTS_TOKEN"
```

- **An entry is an `origin` and exactly one of `sign_in` and `key_env`.** The origin is
  an origin with nothing after it, as an approval's is. At most eight entries
  (`MODEL_TOKENS_LIMIT`), each origin once, compared as origins
  (`https://HuggingFace.co:443` is `https://huggingface.co`). Nothing of what is written
  is shown by a refusal
  (`file::a_downloads_token_is_named_for_an_origin_as_the_sign_in_or_a_variable_and_the_sign_in_for_the_hub_alone`).
- **`sign_in` has one value, `huggingface`, and is taken for `https://huggingface.co`
  alone** (`SIGN_IN_ORIGIN`). That origin is the one the Hub's token is ever sent to:
  a config that names the sign-in for any other, `http://huggingface.co` and another
  port included, is refused, so no config can send the owner's token elsewhere.
- **`key_env` names a variable of the env file,** read as every key is ("The env
  file"): it is among the variables the config names (`Config::variables`), so a config
  that names one is refused without an env file, its value is one of the executor's
  secrets, a value under eight bytes is refused by the variable's name, and one the file
  does not set is listed as missing. `Credentials::model_token` hands it for its origin
  and no other.
- **Like the rest of the `models` table, it is in no digest:** where a token is says
  nothing of what a run means.

**Where the sign-in is.** The binary's `main` has the one function that names the real
place (`hub_sign_in_token_file`), and hands it on uncalled; `just check` refuses its
name anywhere else (`scripts/check-sign-in.sh`). It reads four variables of the
process's environment and hands them to `whizbang::sign_in::token_file`, which reads
nothing itself, and which every test gives made-up folders. The order is the Hub's
documentation of its environment variables
(`https://huggingface.co/docs/huggingface_hub/package_reference/environment_variables`,
read for C0b's pull request and again for this one; no token file was looked at):

1. `HF_TOKEN_PATH`, the token file itself, when it is set;
2. else `token` in `HF_HOME`;
3. else `huggingface/token` in `XDG_CACHE_HOME`, which the Hub uses "only when
   `HF_HOME` is not set";
4. else `.cache/huggingface/token` in `HOME`.

**Why all three that move it, and not only the default.** D12 has whizbang read the
sign-in the Hub's tool keeps, and that tool writes the token where these say: a person
who set `HF_HOME` signed in there, and a reader that looked only under the home folder
would say the sign-in is not there when it is, or read an old token left in the
default place. A variable that is set and empty is as one not set. A variable that is
set to a path that is not absolute names no place at all, and the sign-in is then not
there: it is never taken as the next one down, since a sign-in read from another place
than the one the person named is not theirs, and `~` is not read as a home folder
(`sign_in::tests::the_place_is_hf_token_path_then_hf_home_then_xdg_cache_home_then_the_home_folder`).

**`HF_TOKEN` and `HUGGING_FACE_HUB_TOKEN` are not read.** The Hub's own tools take
either in place of the file. The owner chose the sign-in, and whizbang's other way in
is the env file's key; a token in the binary's environment would also reach every
program the binary starts. No source of the binary names either
(`models::the_real_binary_started_with_hf_token_set_reads_the_sign_in_and_never_the_variable`).

**What is read** (`sign_in::read`). One file, opened once; what is asked of it is asked
of the file that was opened, not of the name again.

- A link is followed, as the Hub's tool follows one; what it leads to must be a
  regular file. **Nothing else is opened** (from the security review of N4): the
  name is looked at first, a link followed, and a FIFO, a socket, a device or a folder
  is refused before any open, since opening a FIFO waits for a writer that may never
  come. The open itself does not wait (`O_NONBLOCK`, written in the reader for macOS
  and Linux, as the workspace has no crate that names it), so a name swapped for a
  FIFO between the look and the open is opened at once and refused by the look at
  the opened file
  (`credential::a_pipe_at_the_sign_ins_place_is_refused_before_it_is_opened_and_nothing_waits`).
- **The opened file is this user's** (from the same review): its owner is the user
  the process runs as, learned the one way the workspace learns it
  (`store::fs::this_user`, now public), or it is refused. A link planted at the name,
  or a file another user or the system made, may hold another's token, which whizbang
  would send under this user's name, the reason below for the mode
  (`sign_in::tests::a_token_file_that_is_not_this_users_or_that_others_may_read_or_write_is_refused`).
- **Its folder is not one others can write** (`store::fs`'s `who_may_write`): whoever
  can write the folder can put another file at the name. The folder is the one the
  name is in; the Hub's tool makes it 700. A link at the name is followed for the file
  and not for this look; the owner check holds the file the link leads to.
- **No default.** Without a `[[models.tokens]]` entry for the origin nothing is read:
  a default would read the Hub's token of anyone who ran `get`, without their saying
  so (the coordinator's decision for N5). The refusal ends with the three lines to add,
  each printed as a line of its own.
- **A file that others than its owner can read or write is refused, not read and
  warned of.** The Hub's tool writes the file for its owner alone, mode 600, and its
  folder 700 (its source, `utils/_auth.py`, `_write_secret`, read for this record), so
  a sign-in made by the tool passes. One that others can read is a token that may have
  been read already, and one that others can write may be another's token, which
  whizbang would then send under the owner's name. A warning on the standard error is
  seen once and the token sent anyway; a refusal says what to do, `chmod 600` or sign
  in again, and costs one command.
- At most 512 bytes (`TOKEN_MAX_BYTES`), and no more than that and one byte is read.
- The token and at most one line break after it, `\n` or `\r\n`, which is taken off:
  the Hub's tool writes none, and a file written by hand with `echo` has one. Anything
  else that is not printable ASCII, a space included, is refused; so is an empty file
  and a token shorter than a secret (eight bytes), which redaction could not protect.
  The Hub's own reader takes off every line break and the white space at both ends;
  whizbang refuses what that would hide rather than send a token it had to guess at.
- The bytes read are overwritten with zeros before the reading ends.
- A refusal says what is wrong in fixed words and how to sign in or which key to set
  instead, and quotes nothing of the file and nothing of its path
  (`credential::a_sign_in_that_is_not_there_or_cannot_be_read_ends_the_install_before_anything_is_asked_and_its_refusal_holds_no_path`).

**When.** Only `whizbang models get` reads it, only of a model whose entry says a
download from that origin needs a token, and only under a config that names the
sign-in for that origin: after the model was found, runs on this platform and has its
adapter, and before the models directory is opened or anything is asked
(`credential::the_sign_ins_store_is_not_opened_by_a_command_that_downloads_nothing_from_its_origin`).
One limit: a `get` of a model installed already reads it too, since whether anything
is to be downloaded is known only under the model's lock. What is done with the token
is ADR 0012's, "A download's token".

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
on the command line as the config is, by `--env-file <file>`, which is taken only
beside `--config` and came with the binary's wiring too. The
code is `config/env.rs`, and its tests are `env::`.

**The order, and the way in.** `Loaded::read` takes both paths, the config's and the
env file's, and is the way in: it keeps the order below, so that no caller can read a
config without the secrets of its env file, and it answers the config, each backend's
credentials and the executor's secrets of one reading
(`env::one_call_reads_the_env_file_first_and_then_the_config_with_its_secrets`). The
three steps it takes are public too, for a test and for a reader that has its bytes
from elsewhere, and a caller of them keeps the order itself. With no env file the config
is read with no secrets: nothing is known to look for, and every variable it names is
missing.

The env file is read first, whole, by its own strict reader
(`EnvFile::read`, `EnvFile::parse`). Every value in it of 8 bytes or more is then a
secret the config's reader is told of (`EnvFile::secrets`), whether or not the config
names its variable; the config is read once with that set; and the variables the config
names are resolved from the same parsed file (`EnvFile::credentials`). So a value that
sits in the env file is never said back from a config, also when it was pasted under a
name nothing uses
(`env::the_env_file_is_read_first_and_a_config_that_holds_any_of_its_values_is_refused`).
A file that sets a variable to something that is not a secret, and is 8 bytes or longer,
makes a config that holds the same text unreadable; an env file is for keys. A value of
the env file is found in a config in the forms redaction searches for, which since ADR
0014 are the value as it is, in UTF-16, inside base64, percent-encoded and behind the
escapes of a JSON string; written there in a form that record accepts as not found (its
letters in the other case, hex), it is not found.

**No set of secrets is made by leaving a refusal out.** The secrets a config is read
with are made when the env file is read, and the executor's when the credentials are;
if either set cannot be made, that is a refusal of the file (`EnvRefused::Unprotected`)
and never an empty set, which would read a config that holds a key and hand a key out
with nothing to take it out of what is written. A value's length is counted once, on
the value as it is kept, which has no white space at its ends, so it is the length a
set of secrets counts too, and no value the reader keeps is one the set refuses
(`env::a_padded_value_does_not_empty_the_secrets_a_config_is_read_with`,
`env::a_padded_value_does_not_empty_the_secrets_the_executor_takes_out`,
`env::a_padded_value_does_not_let_a_pasted_key_be_said_as_a_variables_name`). Nothing
the reader keeps makes a set refuse, so the refusal is tested at the one place a set is
made (`protected`, by a test inside `config/env.rs`,
`tests::a_set_of_secrets_that_cannot_be_made_is_a_refusal_and_never_an_empty_set`),
with a value the reader would not have kept. And the least length in
`EnvFile::secrets`, lowered by one, keeps such a value and is refused by that place,
which fails `env::a_short_value_is_refused_with_the_variables_name_and_nothing_of_the_value`.

**A line** is empty, a comment that starts with `#`, or `NAME=value`, with `export` and
a space before it if it likes. A name is a variable's name, as in the config. The value
loses the white space around it, a carriage return among it, and then one pair of
single or double quotes: ADR 0005 left the quotes to whoever reads the file, and here
they are taken off, so the bare key is what is handed over and what is taken out of
what is written
(`env::a_quoted_value_a_line_break_and_an_export_prefix_each_give_the_bare_key`). What
is inside quotes is the value, all of it: white space at an end inside the quotes is
not taken off, since that would hand over something other than what was written, and is
refused; so are quotes around nothing
(`env::white_space_at_an_end_inside_quotes_and_quotes_around_nothing_are_refused`). A
variable set to nothing, with no quotes, is as one not set.

**What is refused,** each with the number of its line and nothing of the line, since a
line may be a key pasted where it does not belong: a line that is not an assignment; a
name given twice; a value that cannot be taken for one thing (a space outside quotes, a
quote that is not closed or not at the ends, a backslash inside double quotes, white
space at an end inside quotes, quotes around nothing, more than 4,096 bytes). And of the file, as of the config: what is not a file, and more than
64 KiB, each refused before anything is opened, with no more than the bound and one byte
asked of what was opened
(`env::no_more_than_the_bound_and_one_byte_is_asked_of_what_an_env_file_is_read_from`);
bytes that are not UTF-8, or an ASCII control character; more than 256
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
  and is no refusal of the file. The backend that names it is not ready ("What a config
  offers").
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
  `KeyValue` that it is one; the value is reached by `KeyValue::expose` alone. (A
  `Credential` of ADR 0012, bound to an origin and a header, is made from one by the
  backend's kind, which knows both.) A
  marker key read this way is in no refusal, no `{:?}`, and no file of the store of a
  run that was given it as an argument and failed with it in its message
  (`env::a_marker_key_is_in_no_error_no_debug_no_record_and_no_journal_of_a_run_that_used_it`).

**What the env reader does not do,** each pinned by
`env::what_the_env_reader_does_not_do_is_known`: it expands nothing (`$NAME` in a value
is those letters) and reads no escape; a `#` with no space before it is part of the
value, and one after a space makes the value two things, which is refused; it does not
read the process's environment, so a variable set there and not in the file is missing;
it does not take a byte-order mark off the head of the file, whose first line is then no
assignment and is refused; it follows a link to a file and reads what the link leads to;
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
      2. its model, its provider, its address (its origin as an origin writes itself,
         then its path as given), the name of its key's variable and the
         name of its token's variable, in that order, each `b` and each of which may be
         absent;
      3. `u`, how many settings. Then for each, sorted by its key: `b`, the key; then
         `u` 1 and `u` the whole number, as the 64 bits of its two's complement; or `u`
         2 and `u` 0 for false or 1 for true; or `u` 3 and `b` the text.
2. The cap of one call: `u` millionths of a dollar, which may be absent.
3. `u`, how many recipients. Then for each, sorted by its name:
   1. `b`, its name; `b`, its origin, as an origin writes itself; its account, `b`,
      whole, which may be absent;
   2. `u`, how many upstreams, and each as `b`, sorted;
   3. `u`, how many classes, and each as `b` its name, sorted;
   4. `u` 1 if it may be probed, else 0.

**The `models` table is in no digest** (task N3b): neither its directory nor its least
rate of a download is hashed, whatever it gives
(`digest::the_models_table_is_in_no_digest`). Where the models directory is, and how
slow a download may be, are not part of what a run means: a run's model is the identity
of its installation (ADR 0017), wherever its folder is. With them in it, as task N3a
first had them (a fourth field, hashed when the table gave a key), a config that only
moved the directory changed its digest, and so made stale every call prepared under it
before, and a request sent with such a preparation was refused where it should have
been run, or answered with the run it had made
(`wired::two_configs_that_differ_only_in_where_the_models_directory_is_repeat_the_same_run`).

The short form enters as what it is: a backend named `default`, with the default
`default`. The empty config hashes 45 bytes: the 17 of `whizbang:config/1`, a zero byte,
and three fields of `u` 0, nine bytes each. Three digests are fixed
(`digest::the_digests_of_three_fixed_configs_are_the_records`; they were worked out from
this section alone with a few lines of a scripting language's standard library, not
from the code). Task N3a fixed a fourth, of a `models` table alone
(`sha256:ee9df2ae…`); since task N3b that table hashes as the empty config does, and
the test that held the fourth holds that instead:

| Config | Digest |
|---|---|
| The empty one | `sha256:9d60a543ea39be1ca1bc2c4e1a90094a59ded6bdcfdf1c21436f4026f207c4a6` |
| The example of `SPEC.md` section 4 | `sha256:e0f7ca76571f20c077084f4dcb18a57c0adf857b37d59928546508937fa31c7f` |
| The config `FULL` of the digest's tests, with something in every place 1B had | `sha256:c151bf69e75bfec361a2018365535e49731847d0dbf6d13f4bb71b06356d6399` |

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
- **A change to any setting changes it:** every key of every table but `models` is in
  the fields above, with its place marked by a count or a length, so no two configs run together
  to the same bytes (`digest::a_change_to_any_setting_changes_the_digest`,
  `digest::where_one_text_ends_and_the_next_begins_is_in_the_digest`).
- **No secret is in it.** A variable is in the digest by its name. Its value is in no
  config, so a key that is rotated under the same variable changes no digest
  (`digest::a_variable_is_in_the_digest_by_its_name_and_a_value_by_nothing`). A key the
  reader was not told of, written as a model or into a setting's text, is read like any
  text and hashed with the rest; the digest does not show it.
- **Nothing of the env file is in it, and that stands.** Task I6 pinned what follows
  and left the question to task C4: a call prepared before a key is rotated is admitted
  after it, with the new key and no `Stale::Config`
  (`snapshot::known_a_key_rotated_in_the_env_file_gives_a_new_backend_under_the_same_digest_and_no_stale_config`,
  in `tests/it/contract/`, left as it was). Decided with the source over the files, for
  four reasons:
  - **A key is not a setting.** The digest says which settings a run was made under,
    and a prepared call's fingerprint holds what was asked and what answers it: the
    model, its version, its bound. A rotated key changes none of those. A caller told
    that its preparation is stale because a key changed would prepare the same call
    again and be shown the same thing.
  - **The digest is written down.** It is in every run's record (`asked.config`) and
    in every prepared call a caller keeps, which leave the machine. Whatever of the
    env file went into it could be tested against it by anyone who holds a record: a
    guess at a key, hashed with the config, which is not a secret and which a
    record's settings show part of, either matches or does not. A long random key survives that; the reader takes a
    value of 8 bytes, and a short token would not. A hash with a secret of the
    machine's own mixed in would close that, and would give two machines with one
    config two digests, which "The digest" turned down for the file's bytes.
  - **What is in the env file and is not a key is in the config already.** Which
    variables are named is hashed, by name. The one other thing a call depends on is
    whether a variable is set, which is whether its backend is ready; and that is
    asked again at every admission, of the files as they are then. A call prepared
    while its variable was set and admitted after it was taken out is refused, as
    stale by its model when the request says what it expects and as not ready when it
    does not, under the same digest and with no run
    (`source::a_variable_taken_out_of_the_env_file_after_a_call_was_prepared_refuses_the_call_under_the_same_digest`).
  - **Nothing runs with a stale key.** A backend is made for each snapshot, with the
    key of that reading, and a run is handed the backend of the snapshot it was
    admitted under; so the run after a rotation sends the new key, with or without a
    change of digest.

  What this costs: a record does not say which key a run used, nor that one was
  rotated between two runs.
- **An origin is hashed as an origin writes itself** (ADR 0012): the scheme and the
  host in lower case, and the port only when it is not the scheme's own. A backend's
  address is its origin so written and then its path as the file gave it. So
  `https://h.example`, `https://h.example:443` and `HTTPS://H.Example` are one digest,
  and a path's own letters are not folded. A slash at the end of a backend's address is
  part of its path: `https://h.example` and `https://h.example/` are two addresses and
  two digests (`digest::the_ways_of_writing_one_origin_have_one_digest`). Until this was so, for one
  day, the three were refused or were two digests; no digest of a config had been
  stored.

A key added to the file later that is absent from older files must hash as absent, or it
is a new kind, `config/2`, and every digest changes with it. **The `models` table's
keys are hashed not at all** (task N3b, where task N3a hashed them as a fourth field
when the table gave one): so "a change to any setting changes it" is true of every key
of the file but those two, which are not a run's settings. It follows that a guess at
the path of a models directory cannot be tested against a digest, and that the
directory a flag names and the one the config names are alike in no digest, as the
store a flag names is in none.

### Which settings a run records

`asked.settings` of a run record (ADR 0005) is decided here. `Config::settings_of` makes
them, from the names of the backends a call resolved and the executor's secrets. A
config is the `Settings` of a snapshot (ADR 0015): a run admitted under a snapshot that
was given one records them, and the source over both files gives every snapshot its
config. A run records, as names and values sorted by name:

- for each backend its call resolved, `backends.<capability>.<name>.kind`, and `.model`,
  `.provider` and `.url` where the file gives them, and each of its settings as
  `backends.<capability>.<name>.settings.<key>`;
- `spend.call_cap_micros`, the cap in millionths, when the file gives one.

It does not record the name of a variable, any credential, a backend the call did not
resolve, or the `egress` table: where data went is the record's `egress` group, and the
config's digest in `asked.config` stands for the rest
(`settings::the_settings_of_a_run_are_the_resolved_backends_and_the_cap_sorted_by_name`).
Nor the `models` table (task N3a): a record leaves the machine, and where the models
directory is names a folder of its user's; what a run was answered from is in the
record as the identity of the installation, which is its model's version. The least
rate of a download is of no run
(`file::the_models_table_names_the_directory_and_the_least_rate_of_a_download`).
A `url` is the address as the config keeps it: its origin as an origin writes itself,
and its path as the file gave it. A key the reader was not told of, written as the host
in capitals, is kept in lower case, which redaction does not find.

**Each pair is one a record takes** (ADR 0013, "A setting"), made as a record makes a
setting: a text has the secrets taken out while it is whole and is then cut at the 256
characters a record keeps, so a key the config's reader was not told of, written into a
model or a setting's text, is not in a pair
(`settings::a_settings_text_has_the_secrets_taken_out_whole_and_is_then_cut_as_a_record_keeps_one`).
What a record would not take is never left out in silence: the whole is refused
(`SettingsRefused`), naming the setting and never its value
(`settings::what_a_record_would_not_take_is_refused_naming_the_setting_and_never_its_value`):

- a whole number whose digits, as they are written, hold a secret, the cap among them;
- a name that holds a secret, also across a key of the file and a word the name is made
  with, which is not shown;
- a name longer than the 128 characters a record takes. No config that was read gives
  one, since the reader refuses a backend's name and a setting's key that are too long
  together ("What the file holds"); the refusal is kept, and held by a test inside
  `config/settings.rs` on a config made by hand
  (`tests::a_name_longer_than_a_record_takes_is_refused_and_shown_cut_should_a_config_ever_hold_one`,
  which is what the test above held of such a name until the reader refused it).

A backend that is named and is not one the config gives is refused without its name,
which did not come from the config
(`settings::a_backend_the_config_does_not_give_is_refused_and_its_name_is_not_shown`).
More than the 96 settings a record lists are refused, which three backends of 32
settings each come to; and more names than a config has room for backends, 48, are
refused before any is looked for
(`settings::more_settings_than_a_record_lists_and_more_backends_than_a_config_holds_are_refused`).

### Kinds

A backend's `kind` names the code that answers. A kind is registered by its name as an
action is (`Kinds::register`, as `Registry::register` takes an action), by code the
binary and the tests share: a name is registered once, keeps the rule of an action's
name, and a build has 64 at most. What is given as a name and is none is refused and not
shown (`roster::a_kind_is_registered_once_by_a_name_and_a_build_has_no_more_than_its_bound`).
The core names no kind, and the shipped binary registers none: no network client is
built at this checkpoint. The test kit has a fake (`whizbang_testkit::kinds::FakeKind`).
(Changed by task P6a of checkpoint 1C: the binary registers one kind, `pack`, below.)

A kind is shown one backend at a time (`Kind::make`, of a `Table`): the capability,
the name, the backend's table, and the credentials that table names. It makes the
backend (`Made`: a segmenter, a filler or a judge), which says of itself what it
declares (ADR 0006: its model and features; ADR 0010: its price; ADR 0012: who
receives its requests); or it answers why it does not take the table (`KindRefused`): a
sentence that is the kind's own, or the egress types' refusal of a credential. Both
are meant to be fixed words: the sentence is a constant in the kind's code. The type
does not enforce that against a kind that leaks a made text; what a config offers
(`Offers::of`) replaces a sentence that holds a secret of that config's credentials
with fixed words and cuts it. The backend's own key is among those secrets: the whole sentence is looked at first, one that holds a
secret is replaced with `KIND_SENTENCE_WITHHELD`, and any other is cut at 256
characters (`KIND_SENTENCE_LIMIT`)
(`roster::a_kinds_sentence_that_holds_its_backends_key_is_replaced_with_fixed_words_and_a_long_one_is_cut`).

**A credential leaves a kind bound to one origin.** `KeyValue::carried_to` makes the
`Credential` of ADR 0012 from a key, the address the kind sends to, and a header's name
that is the kind's own constant, with the executor's secrets for that constructor's
look at the header's name. It is bound to that address's origin, so a request to
another backend's address is refused it (`NotAllowed::Elsewhere`), the same host on
another port included. A kind is shown the credentials of its own backend and of no
other: the one place that asks, `Offers::of`, asks `Credentials::of` by the backend's
capability and name
(`roster::a_kind_is_handed_its_own_backends_credential_bound_to_its_address_and_no_others`,
with two kinds, three backends and four keys). A value an env file may hold and a
header may not, as a letter outside ASCII, is the egress type's refusal, and the
backend is not ready
(`roster::a_credential_that_cannot_be_carried_makes_its_backend_not_ready_and_shows_nothing`).

A kind makes the backend and does not declare it apart: what a config offers of a
backend is what the backend that was made declares, so a model a call resolved is
always held (ADR 0015, "One snapshot of the config"). A backend is made each time a
config is offered, from the table and from nothing else: nothing is sent, and nothing
is read from a disk.

**A kind whose backend names a recipient must not declare a version written as a bare
identity,** `sha256:` and 64 lower-case hex digits. That text as a model's version
means a local model under an installation, to the executor and to a roster over an
inventory, and a backend that names a recipient and declares one is handed to no run
(ADR 0015, "An inventory", its limits). Nothing refuses such a kind when it is
registered: the rule is the kind's to keep, and a provider's own digest of a model is
written another way, with a prefix of the kind's.

### The kind `pack`

Added with task P6a of checkpoint 1C (`whizbang::pack`; plan 1C, D4 and D7). The first
kind the binary registers (`wiring::kinds`): a local segmenter whose model is answered
by the pack's worker, through the one supervisor of the process. Its table:

- **The capability `segment`, and no other.** A table under `fill` or `judge` is refused.
- **`model`, required:** a model's name as the models directory has one (ADR 0017). The
  model's features are what the entry for it in the catalogue the wiring is over states
  (ADR 0017, "The catalogue"); a model that catalogue does not hold is refused, since
  there is nothing to declare.
- **No `url`, no `provider`, no `key_env` and no `token_env`.** A backend of the kind is
  local: it declares no recipient and no price, so a call to it may cost nothing and is
  admitted without the ledger (plan 1C, D16), and nothing of it leaves the machine.
- **Its settings** (`settings`), and no other:

| Setting | Is | Rule | When not given |
|---|---|---|---|
| `threshold`, `pad`, `min_cover` | A model's setting: rascal's adapters took these (`docs/carry-over.md`) | A text of a number from 0 to 1, digits with one point at most, as `"0.2"`: a setting is never a number with a fraction | Not sent; the adapter's own |
| `outline` | A model's setting, rascal's rule for an outline | A text | Not sent; the adapter's own |
| `call_limit_s` | The kind's limit for a call (D7) | A whole number of seconds from 1 to 120, the supervisor's own limit for a call | 120 |
| `max_lifetime_s` | The lifetime, `--max-lifetime-s`, of a worker a request of the backend starts | A whole number of seconds from 1 to 31,536,000, a year, the most the worker takes | 86,400, a day |

The model's settings travel in its `load` (ADR 0020) and are no field of a call; a text
of one is also held to the protocol's rule for a setting's text (printable ASCII, no
space at an end, no `/` and no `\`), so that no `load` is refused when it is written. The
kind's own two are not sent: the limit of a call bounds its `segment`, and a worker
already running keeps the lifetime it was started with. Each refusal is a constant of
the kind's code, and the reason a call is given ends with it
(`pack::a_setting_the_kind_does_not_know_is_refused_when_the_config_is_read`). The table
is shown to the kind at each reading of the config, so a setting is refused where the
config is read for a call, before any worker is started: the reader itself takes the
file, since what a kind's settings are is the kind's.

Every test that starts a worker through the wiring sets `max_lifetime_s`, so that a
worker nobody waits for ends by itself
(`pack::a_worker_started_through_the_wiring_is_given_the_lifetime_its_config_sets`).

### What a config offers

`Offers::of` takes a config, the credentials of its env file and the kinds of a build.
For each capability it offers every backend the file gives it, under its name, and the
file's default; the short form is one backend named `default`, which is the default.

- **A backend that is ready is in a `Roster`** (ADR 0006), as its kind declared it, and
  resolution is that record's: the call's choice, or else the default, then the
  features, and never another backend
  (`roster::two_capabilities_with_several_named_backends_resolve_a_choice_and_the_default`,
  which compares the roster with one built by hand).
- **A backend that is not ready is still offered.** It is kept beside the roster with
  its reason (`NotReady`), and a call that chooses it, or that chooses none where it is
  the default, is answered with that reason (`Offers::resolve`, `Unanswered::NotReady`;
  and as prepare asks, `Backends::resolve`). It is never left out, and no other backend
  answers in its place, also when another is ready and declares every feature the call
  needs. Three things make a backend not ready:
  - **its kind is not one the build has**
    (`roster::a_backend_whose_kind_is_not_registered_is_offered_as_not_ready_and_never_as_another`);
  - **its kind does not take its table**, in the kind's fixed words
    (`roster::a_backend_whose_kind_does_not_take_its_table_is_not_ready_with_the_kinds_own_words`);
  - **it names a variable the env file does not set.** This record had left that to the
    backend; it is decided here, for every kind. The reason names the variable, the
    key's first when neither is set, and its kind is not asked
    (`roster::a_variable_the_env_file_does_not_set_makes_its_backend_not_ready_naming_the_variable`);
  - **its kind made a backend of another capability** than the one its table is under
    (`roster::a_kind_that_makes_a_backend_of_another_capability_makes_its_backend_not_ready`).
- **A reason is fixed words, the capability's name and, for a variable, its name.** It
  holds no backend's name, no kind's name, no setting and no value: a variable's name
  kept the rule of one and was looked at for a secret when the config was read. A key
  that is read as a variable's name ("What the reader does not do") is said as one.
  `Offers` prints how many backends are ready and how many are not.
- **What is offered holds each ready backend, made,** under its capability and its
  name, and is what a snapshot holds as its backends (`Offered`, ADR 0015)
  (`roster::what_a_config_offers_holds_each_ready_backend_made_under_its_name_and_no_other`).
  Two offers are equal when they offer the same; the backends they made are not
  compared, since a backend has no equality and each reading makes its own
  (`roster::two_offers_are_equal_when_they_offer_the_same_whatever_backends_they_made`).
- **The roster alone knows only the ready ones.** Asked without the offers around it,
  it tells a call that chose a backend that is not ready that none of that name is
  offered, and a call that chose none where the default is not ready that there is no
  default. Neither is answered by another backend; the reason is the offers' to give.

**Over an inventory** (`Offers::over`, `Files::over`; with task W1, ADR 0015, "What a
config offers, over an inventory"): where prepare asks, a local backend that is ready
is answered under the identity of its model's installation, or is not ready for the
inventory's reason; a reason above comes first. A backend that names a recipient and
declares an installation's identity as its version is not ready, with an inventory or
without (ADR 0015, "One rule for a backend that names a recipient and declares an
identity").

Two backends of one capability that declare one model are both offered
(`roster::two_backends_of_one_capability_that_declare_one_model_are_both_offered`). A
run under a snapshot is handed its backend by the name its call resolved, never by its
model, so the two are told apart (ADR 0015;
`snapshot::two_backends_of_one_capability_that_declare_one_model_are_told_apart_by_name`,
in `tests/it/contract/`).

### Both files, each time

`Loaded::of_files` reads both files as `Loaded::read` does, for an executor that is to
run under what they say. It is a function and keeps nothing: each time it is asked it
reads both again, whole, and answers what they say now, or the refusal. A file that
does not parse refuses that ask, with no path and none of its words, and the ask after
it is put right is answered; a key rotated in the env file is the one the next ask
hands out, with the secrets of that reading
(`files::both_files_are_read_again_each_time_and_a_file_put_right_is_read`).

**With no env file, a config that names a variable is refused** (`LoadRefused::NoEnvFile`),
in a fixed sentence that says the env file is needed. Nothing is protected without one:
a key pasted into such a config would be read, and said back, since nothing told its
reader of one. A config that names no variable is read with no env file. Such a config
is read with no secrets: a key pasted into it is not known, and is kept out of what a
run records only if the executor's secrets hold it. A config that is itself refused is
refused as `Loaded::read` refuses it, before this. `Loaded::read`
is as it was, and reads such a config with every variable missing: the refusal is this
reader's
(`files::with_no_env_file_a_config_that_names_a_variable_is_refused_and_one_that_names_none_is_read`).

**For a reader that kept what it read before,** `Loaded::of_files_knowing` takes the
secrets of earlier readings too, and tells the config's reader of them beside the
values of the env file as it is now. The source over both files is that reader.

### The source over both files

`Files` (`config/source.rs`; tests `source::`, each through an executor a test builds
over the source) is the `ConfigSource` of ADR 0015 over a config file, its env file
and the kinds of a build. It is made with the two paths and the kinds, and reads
nothing until it is asked. Each time it is asked it reads both files again, whole
(`Loaded::of_files_knowing`), and answers one snapshot of what they say, or a refusal.

**What the snapshot holds:**

| Of the snapshot | Is |
|---|---|
| The digest | The config's ("The digest") |
| The backends | What the config offers with the build's kinds ("What a config offers"): each ready backend made, each that is not with its reason |
| The secrets | Every credential the config names that has a value, of that reading, given in one set (`Snapshot::with_secrets` replaces what a snapshot held, so they are given once) |
| The cap of one call | `spend.call_cap_usd`, in millionths, when the file gives one; none when it does not |
| The policy | What the `egress` table's approvals make (`DispatchPolicy::from_approvals`, ADR 0019). Always given: a file that names no recipient gives the policy that approves nothing, and the executor's own policy is never used for a run under this source |
| What makes a run's settings | The config ("Which settings a run records") |

The snapshot holds no credential for a recipient (`Snapshot::with_credential`): a kind
makes a `Credential` and has no way to hand it to the snapshot ("What waits").

- **A roster resolves as ADR 0006 says, through the executor.** With two backends of
  one capability and a default, a call that names none is prepared against the
  default and its run is handed the default; one that names the other is handed the
  other; a name the file does not give is answered by nothing
  (`source::two_backends_of_one_capability_and_a_default_resolve_a_choice_and_the_default_over_the_files`).
  A backend whose kind the build does not have is not ready with that reason, as the
  default and as a call's choice, and a backend beside it that is ready and declares
  the same model answers neither
  (`source::a_backend_whose_kind_this_build_does_not_have_is_not_ready_with_the_reason_and_never_another`).
- **A backend changed in the file between a preparation and its admission is
  `Stale::Config`,** on the same executor, and the call prepared again is admitted
  (`source::a_backend_changed_in_the_file_between_prepare_and_admission_is_a_stale_config`).
- **A run goes on under the snapshot it was admitted with.** A run that rewrites the
  config while it executes is still answered by the backend made at its admission,
  and so is the run it calls, which is prepared under the first config's digest; the
  files are read once for the two. The next run is admitted under the file as it is
  then
  (`source::a_run_that_was_admitted_goes_on_under_its_snapshot_though_the_file_changed_while_it_ran`).
- **A file that cannot be read as what it is fails the call, in fixed words.** The
  source's refusal is a `SourceRefused` (ADR 0015), so the call fails as
  `CallError::Config` with no run, a request's id is not taken, and the call after the
  file is put right is admitted
  (`source::a_config_that_does_not_parse_fails_the_next_call_in_fixed_words_with_no_run_and_the_call_after_it_is_put_right_is_admitted`,
  `source::an_env_file_that_cannot_be_read_as_one_fails_the_next_call_and_put_right_it_is_admitted`,
  `source::a_source_that_names_no_env_file_refuses_a_config_that_names_a_variable`).
  There is one sentence for each kind of refusal of either reader
  (`SourceRefused::from` a `LoadRefused`). It names the file as "the config" or "the
  env file" and holds nothing a refusal took from a file or from the system: no key,
  no variable's name, no number of a line, no limit and no path
  (`source::every_refusal_of_either_file_is_said_in_fixed_words_that_quote_nothing_of_it`).
- **The key is named by the reader, to whoever loads the config.** `Files::load` reads
  both files as the source does and answers the readers' own refusal ("What a refusal
  says"): the key from the top of the file, escaped and cut, or the line of the env
  file, and no path. A command that loads a config shows that; a call is told the
  fixed sentence. Task C4's "Done when" asked that the call name the key; ADR 0015
  ruled that a source's reason is fixed words, and the task's words were changed with
  this.
- **The source keeps the credentials of every reading,** for as long as it lives, as
  the executor does (ADR 0015), and may be told of secrets from the start
  (`Files::knowing`). It reads a config with them: a config that holds a key which
  was since rotated out of the env file is refused whole as `HoldsSecret`, where the
  env file as it is now would have let it be read and said back. And a kind is handed
  them, and its sentence is looked at with them (`Offers::knowing`): a kind lives
  across readings, and a sentence that holds a key it was shown before is replaced
  (`source::a_config_that_holds_a_key_of_an_earlier_reading_is_not_used_and_a_kinds_sentence_that_holds_one_is_not_shown`).
  So the reason a call is given for a backend that is not ready holds no key the
  source has read, where it was cleaned with the keys of one reading.
- **It keeps 4,096 at most** (`SECRETS_KEPT_LIMIT`, the executor's bound). A reading
  that would add one over that is refused in fixed words, nothing of it is kept and
  none is let go. The secrets a source is told of from the start are kept however
  many they are: a source told of more than the bound answers a reading that adds
  none, and a reading that adds one to a set that is, or would be, over the bound is
  refused
  (`source::a_reading_that_would_bring_more_keys_than_a_source_keeps_is_refused_and_none_is_let_go`).
- **It prints** whether an env file is named, how many kinds it holds and how many
  secrets it knows: no path and no key (`source::a_source_prints_no_path_and_no_key`).

What follows is task C5's, each on one executor across a change of the file.

- **A cap lowered in the file is the cap the next admission is held to.** The executor
  holds no call to a cap (ADR 0015): the part that does is task S3's (`spend::CallCap`),
  which refuses a call whose estimate is over the cap of the snapshot it is shown.
  With that part, and one of the test's own before it that keeps each cap an admission
  was shown, a call of 1.5 dollars is admitted under a cap of 2, refused with no run
  once the file says 1, and admitted again under 1.5 and under no cap
  (`source::a_cap_lowered_in_the_config_is_the_cap_the_next_admission_is_held_to_on_one_executor`).
- **A recipient taken out of the `egress` table is refused on the next call.** A run
  whose backend sends a prompt is let through while the table approves its recipient
  for prompts; once the recipient is gone from the file, and again once it is there
  for another class only, the next run's send is refused before anything is handed
  over, and its record says so. The executor in that test was built with a policy
  that approves the recipient for everything: the config's stands in its place
  (`source::a_recipient_taken_out_of_the_egress_table_is_refused_on_the_next_call_on_one_executor`).
- **A credential rotated in the env file is the one the next call's backend is
  handed,** by a call that was prepared before the rotation and is not stale ("The
  digest"). Both keys are taken out of everything written after it: the run's
  journal, a diagnostic it writes and its record, and each is refused in a request's
  id
  (`source::a_credential_rotated_in_the_env_file_is_the_one_the_next_calls_backend_is_handed_and_both_are_taken_out_of_what_is_written_after`).
- **A run's `asked.settings` are what "Which settings a run records" says,** of the
  backend its call resolved and the cap, and a setting changed in the file is what
  the next run records. No credential, no start of one, no name of a variable, and
  nothing of the `egress` table is in any byte the store holds after those runs
  (`source::a_runs_settings_are_those_of_the_backend_its_call_resolved_and_the_cap_and_hold_no_credential_and_no_variables_name`).
  Each of the four tests reads a made-up key from its env file and ends by looking
  for it, and for its first 14 characters, in every file of the store.
- **A setting a record would not take fails the admission.** What
  `Config::settings_of` refuses is `CallError::Config` in that refusal's words, with
  no run and the request's id not taken: a cap whose millionths are the digits of a
  key, written with a decimal point so that the reader does not find it ("What the
  reader does not do"), names the setting and not the number; with the cap put
  right the same request runs
  (`source::a_setting_a_record_would_not_take_fails_the_admission_naming_the_setting_and_never_its_value`).
- **An upstream that holds a secret is refused where the config is read, for every
  secret the source knows.** The config's reader looks for the secrets it was told
  of over the file's bytes and makes each origin with them (ADR 0012), and the
  source tells it of every credential it has read and of those it was told of from
  the start (`Files::knowing`); such a config is `HoldsSecret`, and the call fails
  with no run. ADR 0015 had left an upstream to
  each send, where it is still refused for a secret the executor knows and the
  source does not: an executor and its source are to be given the same secrets, by
  whatever builds them (task W1)
  (`source::an_upstream_that_holds_a_secret_the_source_knows_is_refused_where_the_config_is_read`).

**Limits, each stated.**

- **The secrets a source keeps are in memory.** A new process knows the env file as it
  is and nothing before it: a config that holds a key rotated out before the process
  started is read.
- **Only credentials are kept:** the values of variables the config named at some
  reading. A value of the env file under a name no backend used is known to the
  config's reader while it is in the file, and to nobody after it is taken out.
- **Two asks at once each read the files for themselves.** One that began before the
  other kept its credentials reads the config without them; the one after reads with
  them.
- **The two files are read one after the other,** the env file first, and not as
  one thing: an env file written again after it was read is not seen by that ask.
  So a variable that is given another recipient and another value in one edit can,
  for one ask, be read as its old value bound to its new recipient; the same pairing
  is on the disk whenever the config is saved before the env file. A variable that
  changes recipient is given a new name.
  **Considered with task W1, and left so: the source does not read the env file a
  second time and refuse the ask when it changed.** A second reading would rule out
  only a pairing that was never on the disk at one moment. The pairing that matters,
  an old value beside a new recipient, is on the disk for as long as one file is
  saved before the other, and a source that read both files twice would read it
  twice and use it. Two files cannot be saved as one thing, so the rule stays the
  one above: a variable that changes recipient is given a new name. The second
  reading would also cost every preparation and every admission one more read of a
  file, and add a refusal that a person who is editing the env file meets at random.
- **Every ask reads both files and makes every backend,** for a preparation as for an
  admission (ADR 0015). Nothing is kept from one ask for the next but the secrets.

**What waits.**

- **The part that holds a call to the cap in the binary** waited for the wiring and
  is there: the binary's executor is given `spend::CallCap` (task W1; ADR 0015, "The
  binary wired").
- **A recipient's credential in the snapshot.** The executor attaches a credential
  only from its ways (ADR 0015, "A policy before a send, and a checked send"), and a
  snapshot's ways are given one by `Snapshot::with_credential`. A kind makes the
  `Credential` (`KeyValue::carried_to`) and hands back a backend and nothing else, so
  the source has none to give: a backend made from a config sends with no credential
  until a kind can hand one over. No kind sends anything at this checkpoint; it is
  for the first kind that does (`docs/carry-over.md`).
- **The binary's executor built over this source,** with the same secrets given to
  both, waited for the wiring and is built (task W1; ADR 0015, "The binary wired").

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

## Consequences

- Tasks C4 and C5 build the roster, the cap, the policy and a run's settings from one
  value that cannot be made unchecked.
- A backend whose table names a variable needs it set, whatever its kind: a kind that
  could have done without a token cannot say so.
- A config that names a credential is not used without its env file by the reader an
  executor will be given, though the three steps still read it.
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
