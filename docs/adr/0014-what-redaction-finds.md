# 0014. What redaction finds: a secret is read out of its encodings, not matched against a list of them

Status: accepted. Changes ADR 0005 ("Redaction": the four forms, and that it is one
pass; "What redaction does not find": the list)
Date: 2026-11-14
Amended: 2026-11-14, after the review of its pull request, before it merged. Changed:
the three written forms of ADR 0005 are searched for again, beside the readings, and
each kind of encoding is read alone as well as with the other, since a secret can hold
a `%XX` or an escape as its own text; a UTF-16 stretch is left out only for its own
secret's twin; a base64 edge is the neighbouring unit of the reading, whole; one
stretch is noted for a place, however many forms end there; base64 broken by line
breaks is found. Added: what the machine holds; that bytes can come back as no text; a
secret that is the marker's word; the generated test that holds the search.
Amended: 2026-11-14, a second note, with the second part of task C3. Added: "A JSON
body". Changed: the last line of "What is accepted", which said that part was to come.
Amended: 2026-11-14, a third note, after the review of that second part. Changed, in "A
JSON body": a body is read as JSON however deep it is nested; what is said of a
backslash; a text shorter than the longest form is followed in its own room. Added
there: a string whose bytes are not UTF-8; what a string of tiny strings costs. Added,
under "Consequences": two keys that come back as one, and a number that comes back as
a string.
Amended: 2026-11-15, with task W1 of checkpoint 1B. Changed under "A JSON body": one
writer hands its body to `redact_json`, the keeping of an exchange with a judge.
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

Each secret is searched for in up to thirteen forms, as bytes:

- as it is;
- as a JSON string escapes it, and the same with each `/` written `\/`;
- percent-encoded, with every byte that is not a letter, a digit, `-`, `.`, `_` or `~`
  encoded, with the hex digits in upper case and in lower;
- in UTF-16, little-endian and big-endian;
- inside base64, in the standard alphabet and in the one for URLs, for each of the
  three places in a group of three bytes at which it can begin.

The second and third are ADR 0005's written forms. They are kept beside the readings
below because a reading can be wrong about a text: a secret whose own text holds a
`%` and two hex digits, written as JSON escapes it, is not what a reading of
percent-encoding makes of it; and a `%` or a backslash straight before a secret can
take the secret's first characters for its own. A written form is found whatever
stands before it: the JSON form of a secret that begins with a letter an escape ends in
and holds a quote, straight after a lone backslash, is found by no reading and by its
written form alone
(`redact::a_form_of_json_after_a_lone_backslash_is_found_by_its_written_form_alone`,
`redact::a_secret_whose_own_text_holds_an_encoding_is_found_however_it_is_written`,
`redact::after_a_text_that_ends_in_a_percent_or_a_backslash_a_written_form_is_still_found`).

**Base64.** Three bytes are written as four characters, so which characters a secret
makes depends on where in a group it begins. For each of the three places, the form is
the run of characters whose every bit is the secret's. When that run is found, the
character before it is replaced too if the secret did not begin a group, and the
character after it if the secret's bits did not end a character, each only when it is a
character of base64: those two hold two or four bits of the secret beside their
neighbour's. So every character that holds a bit of the secret is replaced, and no
other (`redact::a_secret_is_found_inside_base64_at_each_place_in_a_group_of_three`,
which works out which characters those are from the lengths alone). The character
before, and after, is the unit of the reading beside the form, and is taken whole: an
edge character written as `\/`, as `%2F` or as a `\u` escape goes whole, and no half
of an escape is left
(`redact::base64_inside_another_form_loses_every_character_that_holds_a_bit_of_the_secret`).

**UTF-16.** A secret of ASCII letters in one byte order is also, one byte to the side,
the same secret in the other order with a zero byte of a neighbouring letter. Of two
such stretches of one secret, each the other but for one byte, the one that begins at
an even place in the bytes is kept, since UTF-16 is in pairs of bytes. Any other
stretch is kept wherever it begins: one with no such twin, and one beside the stretch
of another secret, so that telling the executor of a second secret uncovers nothing of
the first (`redact::a_secret_is_found_in_utf_16_of_either_byte_order`,
`redact::a_second_secret_uncovers_nothing_of_the_first`).

### What is read

The bytes are followed as they are, and then as each of these readings gives them,
with every form looked for in each:

| When the bytes hold | They are read |
|---|---|
| a backslash | as the escapes of a JSON string; and that reading again, as escapes and as percent-encoding |
| a `%` | as percent-encoding; and that reading again, as percent-encoding and as escapes |
| both | with both read at once; and that reading again the same way |
| a `+` | as percent-encoding with each `+` a space |
| a line feed or a carriage return | with the line breaks inside base64 passed over, for base64 forms alone |
| a backslash | as escapes and then with the line breaks inside base64 passed over, for base64 forms alone |

So a text is followed once at least and twelve times at most.

- **Percent-encoding:** `%` and two hex digits, in either case, are the byte they name.
- **Escapes:** a backslash and `"`, `\`, `/`, `b`, `f`, `n`, `r` or `t` are the
  character a JSON string means by them; `\u` and four hex digits, in either case, are
  that character, and two of them that are the halves of a pair are the one character
  outside the first plane.
- **Line breaks inside base64:** a line feed, a carriage return, or the two together,
  between two base64 characters, is passed over: it is how `base64`, MIME and PEM wrap
  a long body, at 64 or 76 columns. Only base64 forms are looked for in this reading,
  so two lines of an ordinary text are not joined into a secret
  (`redact::base64_broken_by_line_breaks_is_found_and_ordinary_lines_are_not_joined`).
- Anything else is as it is, and so is what only looks like one of these: a `%` with
  no two digits after it, half a pair, a backslash before another letter.

Each reading is the only one that finds some text
(`redact::each_reading_finds_a_text_that_no_other_reading_does`).

A stretch found in a reading is the stretch of the bytes as they were given that the
reading was made from: the whole of each `%XX` and each escape. One case leaves part
of an escape, and is in "What is accepted": after a lone backslash, the last digit of
an escape that was not read as one can be read as the secret's own first digit.

So percent-encoding is found however it is mixed: the two cases of hex in one value, a
byte left plain beside encoded ones, a byte encoded that need not be, a space as `+`,
and a value encoded twice or three times
(`redact::a_secret_is_found_percent_encoded_in_any_mixture`). A JSON string's escapes
are found whichever the writer used: the short ones, `\/`, and `\u` for any or every
letter (`redact::a_secret_is_found_behind_the_escapes_of_a_json_string`). And one form
inside another is found where a reading reaches it: base64 inside a JSON string that
escapes its slashes, base64 percent-encoded in an address, a `\u` escape inside
percent-encoding (`redact::the_forms_are_found_inside_one_another`).

### Everything is found before anything is replaced

As in ADR 0005, and now across the readings: every stretch any reading found is noted
over the bytes as they were given; stretches that overlap are one stretch; each is
replaced once by `[redacted]`. Two stretches that only touch are two.

`redact_bytes` works on bytes and gives bytes. From bytes that were UTF-8 it can give
bytes that are not: a secret's UTF-16 bytes can stand inside the UTF-8 of other
letters, and what is left of those letters is no character. A writer of text goes
through `redact`, which gives a text, with the mark for what is no character where a
half stood
(`redact::bytes_that_were_text_can_come_back_as_no_text_and_a_text_comes_back_as_one`).

### What it costs

The forms of every secret are one machine that follows a text once and says what ends
at each place (the construction of Aho and Corasick, written in this file). Each state
knows, from when the machine was made, the longest form that ends there, the longest
base64 form and the longest wide one, with what the shorter forms that end there say of
their edges. So following a text notes two stretches at most for a place, whatever the
number of secrets and however many of them end there, and costs the text's length
(`redact::two_hundred_secrets_that_end_inside_one_another_cost_no_more_than_one`: every
place in a mebibyte ends two hundred secrets). A text is followed once, and up to
eleven times more by what it holds. Building the machine costs the summed length of
the forms, once, when the secrets are made; a `Secrets` that is copied shares it.

Measured on the builder's machine, an Apple silicon laptop, in the unoptimised build
the tests run, inside a run of the whole fast tier:

| Case | Time | Test |
|---|---|---|
| 1 MiB that holds `%`, `\`, `+` and line breaks throughout, ten secrets: all twelve followings | 1.5 s | `redact::a_mebibyte_with_ten_secrets_is_searched_inside_a_stated_time` |
| 1 MiB of JSON full of the letters 2,000 keys share, 2,000 secrets (ADR 0005's 3 seconds), building the machine included | 1.2 to 1.3 s | `redact::many_secrets_that_share_their_start_cost_no_more_than_one` |
| 8 MiB with 2,000 secrets and no `%`, backslash, `+` or line break, building the machine included | 0.8 s | `redact::many_secrets_over_a_large_body_are_one_pass` |
| 1 MiB every place of which ends 200 secrets | 0.35 s | `redact::two_hundred_secrets_that_end_inside_one_another_cost_no_more_than_one` |

The tests that state a time ask for it inside 10 seconds, which is loose on purpose: a
test that fails on a busy runner teaches nothing. What they hold is that the time does
not grow with the number of secrets, nor with how many end at one place.

**What the machine holds,** by count and not by measurement: a state for each byte of
each form, less what forms share at their starts, which is about 13 states for each
byte of a secret (one for the secret as it is and its written forms where they are the
same bytes, four for UTF-16, eight for base64); and for each state 8 bytes, 16 for
what ends there, and 5 for the edge that leads to it, in runs and not in a map. That is
about 380 bytes for each byte of secret: some 12 KiB for a key of 32 bytes, and some
24 MiB for an env file filled to its 64 KiB with values. That holds for a key of
letters and digits; a value of marks holds up to half as much again, since its written
forms are then other bytes than the value, and while the machine is made it holds
several times its final size, in a map that is then dropped. The set a config is read with (every value of the env file) and the
set the executor takes out (the credentials the config names) are two sets and two
machines, each made once for a reading of the two files.

### What holds the search

`redact::a_generated_secret_in_each_form_at_each_place_is_replaced_exactly` makes
secrets from a seed, out of the marks an encoding is made of among other letters, and
writes each every way this record says it is found, between texts of several kinds,
some 2,300 cases. What comes back must be the text before, the marker and the text
after, exactly: so nothing of the secret is left in any encoding, and nothing beside it
is taken. It is there for the next change to this file.

### A JSON body

`Secrets::redact_json` takes a body that is JSON and gives back one that is still JSON,
with the secrets taken out:

- **Each string is decoded, searched and written again.** A string, whether a key or a
  value, is read to the text it means, that text is searched as any text is (every
  form, every reading), and when it held a secret it is written again as a JSON string
  with `[redacted]` in the secret's place. So no escape can hide a secret, since the
  search is of the text and not of how it was written; and the marker cannot break a
  string, since the string is written anew around it. The two cases ADR 0005 named, a
  secret that ends in a backslash and one that begins with a letter an escape ends in,
  are not cases here
  (`redact::a_json_body_with_a_secret_at_the_edge_of_an_escape_is_still_json_after_it`,
  `redact::a_secret_behind_any_escape_of_a_json_string_is_taken_out_of_a_json_body`).
- **What holds no secret is left byte for byte:** a string that held none, with
  whatever escapes its writer chose, and everything between the strings: spacing, the
  order of keys, a key given twice, numbers as they were written
  (`redact::what_holds_no_secret_in_a_json_body_is_left_byte_for_byte`).
- **A number, `true`, `false` or `null` that holds a secret becomes the string
  `"[redacted]"`,** whole, so that the body is still JSON
  (`redact::a_number_that_holds_a_secret_becomes_a_string_and_the_body_is_still_json`).
- **A string that is JSON and no text,** as half of a pair of `\u` escapes is, or one
  that holds bytes that are not UTF-8, is not decoded: it is searched as it is written
  and replaced whole when it holds a secret
  (`redact::a_string_that_is_json_and_no_text_is_searched_as_it_is_written`). A body
  with such bytes inside a string is taken as JSON, and what comes back is UTF-8 only
  when what was given was
  (`redact::a_string_of_a_json_body_that_is_not_utf_8_is_searched_as_it_is_written`).
- **A body that is not JSON is not refused.** It is searched as bytes, as
  `redact_bytes` searches: a body cut short, text after the value, a byte-order mark, a
  control character inside a string
  (`redact::a_body_that_is_not_json_is_redacted_as_bytes_and_not_refused`,
  `redact::a_byte_order_mark_and_a_control_character_in_a_string_make_a_body_that_is_not_json`).
  However deep a body is nested it is read as JSON: the reader that is asked keeps
  nothing and does not recurse (`redact::a_body_nested_however_deep_is_read_as_json`,
  at 5,000 levels).

Whether a body is JSON is asked of `serde_json`, which reads it once and keeps nothing;
the strings are then found by their quotes, which in a body that is JSON is exact. A
number or a word ends at the next byte that stands between values, or where the body
ends
(`redact::a_body_that_is_one_bare_number_that_holds_a_secret_becomes_the_marker`). The
whole costs the body's length: 1 MiB of JSON with ten secrets took 0.5 s in the
unoptimised build
(`redact::a_mebibyte_of_json_with_ten_secrets_is_redacted_as_json_inside_a_stated_time`).

**A string costs its own length, not the longest secret's.** Following a text keeps
the last units it read, as many as the longest form is long; for a text shorter than
that it keeps as many as the text is long (`Search::room`, held by a test beside the
code, `tests::a_text_is_followed_in_room_for_the_shorter_of_the_longest_form_and_the_text`:
the room cannot be seen from outside, since a text is redacted the same in any room
that is large enough). Before this, each following of each string made room for the
longest form, so a body of many tiny strings cost with the longest secret. Against one
value of 4,096 marks, in the unoptimised build on the builder's machine: 1 MiB of
strings of four characters that hold every mark a reading is made for, each followed
twelve times, took 3.0 s and takes 1.2 s
(`redact::a_mebibyte_of_tiny_strings_against_one_long_value_is_redacted_inside_a_stated_time`,
which asks for it inside 10 seconds); 1 MiB of strings of one letter took 0.33 s and
takes 0.16 s. On that machine each is now what the same body takes against a value of
eight bytes. That is not so everywhere, and no test asks for it: on CI's Linux runner
the one-letter body took 0.44 s against the long value and 0.25 s against the short
one, with the room as it now is. The machine of a long value is some megabytes, and
why it follows a text more slowly there was not looked into. What is found is
unchanged: the generated test above and every other test of the suite pass as they
were.

**A backslash in a body that is JSON.** A backslash that is the body's own escape is
decoded before the search, so it throws no reading off
(`redact::a_backslash_before_a_secret_in_a_json_body_is_a_character_and_stays`). The
text a string means is then searched as any text is, readings included: a text that
holds a second encoding after a lone backslash is as in "What is accepted", and a
backslash straight after a secret that ends in one goes with it
(`redact::the_text_a_string_of_a_json_body_means_is_searched_with_the_readings_as_any_text_is`).
A secret that only decoding finds is found: one percent-encoded twice in mixed case
and then written as `\u` escapes is a level deeper than the readings of bytes go, and
is two levels deep in the text its string means
(`redact::a_secret_found_only_by_decoding_a_string_is_taken_out_of_a_json_body`). The
function takes the secrets by reference and makes no machine; and each string that is
a text goes through `redact`, as a text, so what is written back for it is a text.

**What it does not find,** pinned by
`redact::what_redaction_of_a_json_body_does_not_find_is_known`: a secret that stands
across two strings, or across a string and what is between strings, is in no one
string. Searched as bytes it would be found, and the body broken; here the body is kept
and the secret, which no one value held, is not found.

**One writer calls it: an exchange with a judge** (with task W1 of checkpoint 1B, which
took the row off `docs/carry-over.md`). A handle writes an exchange as JSON, and the
executor keeps it through `Diagnostic::written_json`, which redacts it as JSON: an
exchange in which the letters of a secret stood after a line break's escape was
stored with a backslash before the marker, and could not be read back as one
(`asked::an_exchange_with_a_judge_is_stored_as_json_after_a_secret_at_the_edge_of_an_escape_was_taken_out`).
So what this section does not find holds for a stored exchange from here on: a secret
that stands across two of its strings. Nothing else the executor writes is a body it
knows to be JSON and searches whole. A record and an admission are made of fields
that are each cleaned as a text before the document is written; a journal is lines
whose messages were cleaned the same way; and what an action writes under a name of
its own is bytes to the executor, whatever they are, and is searched as bytes, as it
was (the same test, its last lines).

### What is accepted

`redact::what_redaction_does_not_find_is_known` pins each of these.

| Not found | Why it is accepted |
|---|---|
| A key the executor was not told of | Redaction is by value; it does not know a key by its shape (ADR 0005) |
| A value percent-encoded four times or more | The readings go two deep, and the written form they then find is the third. Each level more is another following of every text that holds a `%`, for a form no client is known to write |
| A `+` for a space inside a value encoded twice | The reading with `+` as a space is made once, of the bytes as given |
| A mixture that only a reading finds, straight after a `%`, when the secret begins with two hex digits | A reading goes through a text once from its start, and takes `%` and two hex digits for a byte. As it is, and in its written forms, the secret is found there |
| An encoded secret straight after a lone backslash, when the secret is written as `\u` escapes inside percent-encoding, or in mixed escapes and begins with a letter an escape ends in | A reading goes through a text once from its start, and takes the backslash with what follows. As it is, in its written forms, and as `\u` escapes alone, the secret is found there. Where the secret begins with a digit it is found, and the last digit of the first escape goes with it while the rest of that escape stays |
| The letters in the other case; the key as hex digits, or in base32 or another alphabet | Each is one more table of forms or one more reading on every write. None is how a header, an address or a JSON body carries a key |
| UTF-32; base64 of UTF-16; any encoding inside another that no reading opens | The same. Base64 is searched for the secret's own bytes only |
| Base64 broken by anything but one line break between two of its characters: a blank line, a space, a fold | The reading that passes over line breaks would otherwise join texts that are apart |
| A secret split across two texts, or across two lines of a journal | Each text is searched alone (ADR 0005) |
| A secret compressed, hashed, encrypted, or inside an image | Nothing short of undoing each would find it (ADR 0005) |
| A body searched as bytes is not kept valid as JSON | Bytes are searched without a grammar. A body known to be JSON is handed to `Secrets::redact_json` ("A JSON body", below); one searched as bytes can still be left with a backslash before the marker |

Two things that are not misses, pinned in the same test:

- **A lone backslash straight before a secret written as `\u` escapes goes with the
  secret.** The first reading takes it with the first escape's backslash; the second
  finds the secret, and its stretch begins at the lone backslash.
- **A secret that is the marker's own word, or part of it** (`redacted`), is found
  inside the marker when what was stored is redacted a second time, and the brackets
  grow: `[[redacted]]`. Once is right; a text is redacted once before it is stored.
  Refusing such a value where a set is made would need a second kind of refusal in
  `SecretRefused`, which holds a length and nothing else, and is left until something
  redacts twice.

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
  is about 1.2 seconds here, in a build that is not optimised, the machine's making
  included.
- A text with a backslash in it, as nearly every JSON body has, is followed five times
  and not once, and one with every mark twelve times. That is the price of reading it,
  and the table above states it.
- The machine is tens of mebibytes for an env file full of values, where the list of
  forms it replaces was a few times the secrets' own length.
- A `Secrets` keeps its values as text beside the machine, to be compared with another
  `Secrets`; it still prints only how many it holds.
- Two keys that each held a secret can come back as the same key
  (`redact::two_keys_that_each_held_a_secret_can_come_back_as_the_same_key`).
- A stored body in which a number became a string may not be read back by a reader
  that expects the number; that is taken over leaving the digits
  (`redact::a_number_that_holds_a_secret_becomes_a_string_and_the_body_is_still_json`).
- `docs/carry-over.md`: the row on what redaction does not find was cut down to the one
  thing left, that nothing handed a JSON body to `redact_json`, and is gone since an
  exchange with a judge is ("A JSON body").

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
