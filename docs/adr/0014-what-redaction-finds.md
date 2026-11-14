# 0014. What redaction finds: a secret is read out of its encodings, not matched against a list of them

Status: accepted. Changes ADR 0005 ("Redaction": the four forms, and that it is one
pass; "What redaction does not find": the list)
Date: 2026-11-14
Spec: section 3, "What every run records" ("Secrets are redacted before anything is
written"); section 10, "Egress" ("Credentials and tokens are redacted from logs, events,
journals, results and errors")

## Context

ADR 0005 took a secret out of what is written where it stood in one of four forms: as it
is, as a JSON string escapes it, the same with `\/`, and percent-encoded with every
reserved byte encoded in one case of hex. It listed what got through, and a test pinned
each: a secret inside base64, as a basic authorization header holds one; a secret with a
letter written as a `\u00XX` escape; a secret in UTF-16; and five other ways of
percent-encoding (the two cases of hex mixed, a reserved byte left plain, a space as
`+`, a byte encoded that need not be, a value encoded twice). It said the cost of the
pass was the length of the text times the number of forms that share a first byte, and
gave a figure: about 3 seconds for 1 MiB of JSON against 2,000 keys that share their
first letters.

Checkpoint 1B is where a config first holds a key and a backend first sends one. The
plan's task C3 asks, for each form on that list, that it be found or accepted with its
reason.

A list of written forms cannot be finished: a letter can be escaped or not, one at a
time, so a key of 40 letters has more written forms than can be searched for. What can
be done instead is to read the text as its encoding reads it, and look for the secret in
what was read.

## Decision

The code is `crates/whizbang-core/src/contract/redact.rs`, and a test named below is in
`crates/whizbang-core/tests/it/contract/redact.rs`. What ADR 0005 says of a secret's
least length, of trimming, of the empty value and of what `Secrets` prints stands.

### What is searched for

Each secret is searched for in nine forms, as bytes:

- as it is;
- in UTF-16, little-endian and big-endian;
- inside base64, in the standard alphabet and in the one for URLs, for each of the
  three places in a group of three bytes at which it can begin.

**Base64.** Three bytes are written as four characters, so which characters a secret
makes depends on where in a group it begins. For each of the three places, the form is
the run of characters whose every bit is the secret's. When that run is found, the
character before it is replaced too if the secret did not begin a group, and the
character after it if the secret's bits did not end a character, each only when it is a
character of base64: those two hold two or four bits of the secret beside their
neighbour's. So every character that holds a bit of the secret is replaced, and no
other (`redact::a_secret_is_found_inside_base64_at_each_place_in_a_group_of_three`,
which works out which characters those are from the lengths alone).

**UTF-16.** A secret of ASCII letters in one byte order is also, one byte to the side,
the same secret in the other order with a zero byte of a neighbouring letter. Of two
such stretches the one that begins at an even place in the bytes is kept, since UTF-16
is in pairs of bytes; a stretch with no such twin is kept wherever it begins
(`redact::a_secret_is_found_in_utf_16_of_either_byte_order`).

### What is read

The bytes are followed four ways, and every form is looked for in each:

1. **As they are.**
2. **Read once,** when they hold a `%` or a backslash: `%` and two hex digits, in
   either case, are the byte they name; a backslash and `"`, `\`, `/`, `b`, `f`, `n`,
   `r` or `t` are the character a JSON string means by them; `\u` and four hex digits,
   in either case, are that character, and two of them that are the halves of a pair
   are the one character outside the first plane. Anything else is as it is, and so is
   what only looks like one of these: a `%` with no two digits after it, half a pair,
   a backslash before another letter.
3. **Read twice,** when they hold a `%` or a backslash: the second reading read again
   the same way, which is what a value encoded twice needs.
4. **Read once with each `+` a space,** when they hold a `+`: what a form's body does
   with a space.

A stretch found in a reading is the stretch of the bytes as they were given that the
reading was made from: the whole of each `%XX` and each escape, never half of one.

So percent-encoding is found however it is mixed: the two cases of hex in one value, a
byte left plain beside encoded ones, a byte encoded that need not be, a space as `+`,
and a value encoded twice (`redact::a_secret_is_found_percent_encoded_in_any_mixture`).
A JSON string's escapes are found whichever the writer used: the short ones, `\/`, and
`\u` for any or every letter
(`redact::a_secret_is_found_behind_the_escapes_of_a_json_string`). And one form inside
another is found where a reading reaches it: base64 inside a JSON string that escapes
its slashes, base64 percent-encoded in an address, a `\u` escape inside
percent-encoding (`redact::the_forms_are_found_inside_one_another`).

ADR 0005's three written forms beside the secret itself (JSON-escaped, the same with
`\/`, percent-encoded) are no longer searched for as forms: the second reading finds
each, and its tests stand as they were.

### Everything is found before anything is replaced

As in ADR 0005, and now across the readings: every stretch any reading found is noted
over the bytes as they were given; stretches that overlap are one stretch; each is
replaced once by `[redacted]`. Two stretches that only touch are two.

### What it costs

The forms of every secret are one machine that follows a text once and says where each
form ends (the construction of Aho and Corasick, written in this file: a table for the
first byte, a map for the rest, and for each state where to go on from when nothing
matches). Following a text costs its length, whatever the number of secrets. It is
followed once, and three times more when it holds a `%` or a backslash and once more
when it holds a `+`, so at most five times. Building the machine costs the summed
length of the forms, once, when the secrets are made; a `Secrets` that is copied shares
it.

Measured on the builder's machine, an Apple silicon laptop, in the unoptimised build
the tests run, inside a run of the whole fast tier, with the other tests beside each:

| Case | Time | Test |
|---|---|---|
| 1 MiB that holds `%`, `\` and `+` throughout, ten secrets: all five followings | 0.41 s | `redact::a_mebibyte_with_ten_secrets_is_searched_inside_a_stated_time` |
| 1 MiB of JSON full of the letters 2,000 keys share, 2,000 secrets (ADR 0005's 3 seconds), building the machine included | 1.28 s | `redact::many_secrets_that_share_their_start_cost_no_more_than_one` |
| 8 MiB with 2,000 secrets and no `%`, backslash or `+`, building the machine included | 0.80 s | `redact::many_secrets_over_a_large_body_are_one_pass` |

The two tests that state a time ask for it inside 10 seconds, which is loose on purpose:
a test that fails on a busy runner teaches nothing. What they hold is that the time does
not grow with the number of secrets.

### What is accepted

`redact::what_redaction_does_not_find_is_known` pins each of these as getting through.

| Not found | Why it is accepted |
|---|---|
| A key the executor was not told of | Redaction is by value; it does not know a key by its shape (ADR 0005) |
| A value percent-encoded three times or more | The readings go two deep. Each level more is another following of every text that holds a `%`, for a form no client is known to write |
| A `+` for a space inside a value encoded twice | The reading with `+` as a space is made once, of the bytes as given |
| The letters in the other case; the key as hex digits, or in base32 or another alphabet | Each is one more table of forms or one more reading on every write. None is how a header, an address or a JSON body carries a key |
| UTF-32; base64 of UTF-16; any encoding inside another that no reading opens | The same. Base64 is searched for the secret's own bytes only |
| Base64 broken by a line break inside the secret's characters | The forms are runs of characters. A wrapped body would need the reading to drop line breaks, which would join texts that are apart |
| A secret split across two texts, or across two lines of a journal | Each text is searched alone (ADR 0005) |
| A secret compressed, hashed, encrypted, or inside an image | Nothing short of undoing each would find it (ADR 0005) |
| A body searched as bytes is not kept valid as JSON | This record searches bytes. A body known to be JSON is redacted as JSON by the second part of the task, which adds its section to this record; until then a secret that begins with a letter an escape ends in, behind a backslash, still leaves that backslash before the marker |

## Consequences

- A secret in a basic authorization header, in a worker's UTF-16 output, behind a
  `\u` escape or in an oddly encoded address no longer reaches a record, a journal or a
  diagnostic.
- More is replaced than before, on purpose: a base64 character that holds two bits of a
  secret goes with it, and so does a whole `%XX` or escape that a secret's stretch ends
  inside.
- A text that only resembles an encoding of a secret is still as it was: a reading
  that yields the secret's bytes is an encoding of the secret, and no other reading
  changes anything.
- The number of secrets no longer matters to a write. ADR 0005's figure of 3 seconds
  is 1.28 seconds here, in a build that is not optimised, the machine's making
  included.
- A text with a `%` or a backslash in it, as nearly every JSON body has, is followed
  four times and not once. That is the price of reading it, and the table above states
  it.
- A `Secrets` keeps its values as text beside the machine, to be compared with another
  `Secrets`; it still prints only how many it holds.
- `docs/carry-over.md`: the row on what redaction does not find is cut down to what
  this record accepts.

## Alternatives

- **More written forms.** A letter may be escaped or not, one at a time: the forms of
  one key are more than can be listed, which is why five got through a list of four.
- **Decode the whole text, redact, and encode it again.** What is stored would not be
  the text that was given with the secret taken out, and a text that is not valid in
  its encoding could not be stored at all.
- **A library for the search.** The one this would be is not among the workspace's
  dependencies, and the construction is small enough to hold here, with its tests.
- **Refuse to write a text that might hold a secret in a form not searched for.**
  Nothing tells such a text from any other.
