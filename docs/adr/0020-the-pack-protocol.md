# 0020. The pack protocol: `pack/1`, frames of a JSON header and raw payloads, every bound read before the bytes

Status: accepted. Changes ADR 0006: a segment request carries the identity of the
binding its picture is under, an instance names the binding its plane belongs to, and
the handle refuses an answer whose planes name another
Date: 2026-11-15
Spec: section 4, "The Python pack"; section 10, "Two HTTP surfaces"; section 13
Amended: 2026-11-17, with task P7a (ADR 0025). Added: the worker of the pack's core
states `cpu` as its device in its hello, the device its work runs on while no model is
resident, since the core imports no runtime; a hello's device may still be null for a
worker that cannot say. Nothing of the frames changed.
Amended: 2026-11-17, from the review of task P2 (the pack's own codec). No byte and no
reason of `pack/1` changed: whizbang's reader is as it was, and these words state what it
does where the record was silent or said otherwise. Changed: "A text of the protocol"
names its characters and not the cleaning of ADR 0006, which puts `?`; the bounds table
says when a header that is no result's is held to 64 KiB and that the 1 GiB of a result's
payloads is looked at last among what is open; `over_bound` is of a count or size within
the 32 bits its field holds. Added: "The steps in full", under "What a refusal says". Ten
refusal vectors were added, each reason given by whizbang's reader.
Amended: 2026-11-17, by task P3b (one call through the supervisor). No byte and no reason
changed. Added, under "What a refusal says": which `over_bound` ends a call as
`too_large`, what a request whizbang would refuse to write is, how a request from a
worker is refused, and the text of the `error` whizbang sends after a refusal. And under
"Consequences", what a run keeps of a result's predicted IoU and diagnostics.
Amended: 2026-11-17, by task P4 (the queue, cancellation, deadlines and restart). No byte
and no reason changed. Added: "The worker", under "Decision".
Amended: 2026-11-17, by task P6a (the kind `pack`). No byte and no reason changed. Added,
under "The worker": what a caller gives with a request beside its cancellation and its
deadline, the kind's limit for a call and the lifetime of a worker it starts.

## Context

`SPEC.md` section 4 leaves the protocol between whizbang and the Python pack to
checkpoint 1C, within these constraints: a handshake that states the version and refuses
a mismatch; framed messages (request, result, event, cancel, error), each with a request
id; payloads that state dimensions, layout, data type, colour and alpha rules and the
binding they belong to; a byte bound on every message and payload; nothing in it a file
path; one codec for the local worker and one for HTTP, each the simplest that meets the
bounds.

The plan of checkpoint 1C (`docs/plans/m1c-local-pack.md`, decision D2) fixes what the
protocol is, and leaves the bytes to this record. Two rows of `docs/carry-over.md` are
answered here: what a segmenter adapter took and gave beside the segment interface (its
settings, its diagnostics, the order of its answer), and that nothing bounded what is
read off a wire before something is built from it.

What was read, at the commits `docs/sources.toml` pins:

- Rascal's pack client (`crates/rascal-mcp/src/client/mod.rs`) held one answer to a
  ceiling, 256 MiB (`MAX_ANSWER_BYTES`, `crates/rascal-core/src/pack.rs`), by the line
  length of its transport; a picture travelled as base64 inside JSON.
- Rascal's filler (`crates/rascal-mask/src/filler/mod.rs`, `answer_cap`) worked the most
  bytes of an answer out from its request: eight bytes a pixel and a mebibyte.
- Rascal's segmenter adapters (`tools/python/src/rascal_tools/models/adapters.py`) took
  a threshold, a padding, an outline rule and a least cover, gave each instance
  diagnostics and a predicted IoU, and sorted every answer one way (`order`).
- Squint's pack client (`crates/squint-tools/src/mcp/mod.rs`) spoke JSON-RPC, one JSON
  document a line, and routed answers by request id.

Neither old protocol is kept (plan, D1): both were MCP. What is kept is the idea of a
bound worked out from the request, and the one order.

## Decision

The code is `crates/whizbang-backends/src/pack/proto.rs`; the tests that hold it to this
record are in `crates/whizbang-backends/tests/it/pack/proto.rs`; the fixed vectors are in
`pack/conformance/vectors/`. The Python codec (task P2) reads the same vectors.

### The version

One name: `pack/1`. The first frame each way is a `hello` that states it. A side speaks
one version. A hello that states any other is refused, and the refusal says both
versions; there are no ranges and nothing is negotiated.

### A frame

```
frame          = header-length header payload*
header-length  = 4 bytes: an unsigned whole number, most significant byte first
header         = that many bytes: one JSON object, in the canonical writing below
payload        = payload-length samples
payload-length = 8 bytes: an unsigned whole number, most significant byte first
samples        = that many bytes
```

A header describes each payload that follows it by an entry, and the payloads follow in
the order their entries are written in the header. Frames follow each other with nothing
between them. The local codec writes frames on the worker's standard input and reads
them from its standard output; the worker's error output is diagnostics only and is
never read as frames.

Input that ends before a frame's first byte is the end of the conversation. Input that
ends anywhere inside a frame is a refusal (`truncated`), never a shorter message.

### The canonical writing of a header

A header has exactly one writing, so a frame that is read can be written back byte for
byte, and two codecs cannot differ in bytes while they agree in meaning.

- No white space. A field is `"name":value`; fields and items are separated by `,`.
- An object's fields are written in the order this record lists them, each once.
- A text is written between `"`. Inside it `"` is `\"`, `\` is `\\`, and the characters
  U+0008, U+0009, U+000A, U+000C and U+000D are `\b`, `\t`, `\n`, `\f` and `\r`; any
  other character under U+0020 is `\u00` and two lower-case hex digits; every other
  character is itself, in UTF-8.
- A whole number is its decimal digits, with no sign, no leading zero, no fraction and
  no exponent. Only a setting's value may be under zero, and is then written with `-`.
- **A number with a fraction is never a JSON number.** It is written as a text of 16
  lower-case hex digits: the 64 bits of its IEEE 754 double, most significant first. So
  `0.5` is `"3fe0000000000000"`. Two JSON libraries print one double two ways, and the
  vectors could not then be held byte for byte. A value that is not finite is refused.
- `true`, `false` and `null` are written so.

A reader checks this by writing what it read and comparing the bytes: a header that
parses and is not its own canonical writing is refused (`not_canonical`).

### What a reader takes as JSON

A header is decoded as strict UTF-8 and then parsed as JSON (RFC 8259), strictly. Each
of these is refused as `not_json`, before any field is looked at, though a lenient
reader would take it and refuse it later for another reason, or not at all:

- A byte-order mark (`EF BB BF`) before the object.
- Bytes that strict UTF-8 does not allow, among them a surrogate encoded as bytes
  (`ED A0 80` to `ED BF BF`).
- A `\u` escape of a surrogate that is not one half of a pair (`\ud800` alone, or
  followed by anything but a `\udc00` to `\udfff`, and `\udc00` to `\udfff` alone): it
  names no character. A pair is JSON and names one character, which the canonical
  writing writes as itself, so a header that escapes one is not its canonical writing.
- A number outside a double's range, such as `1e400` or `-1e400`.
- Objects and lists nested 128 deep, counting the header's own object as the first. A
  reader takes 127; no message of `pack/1` is deeper than 4.
- A field given twice in one object.
- `NaN`, `Infinity`, `-Infinity`, and anything else that is not a JSON value: a number
  JSON's grammar does not write (`01`, `1.`, `.5`, `+1`, `1e`), an escape it has not
  (`\x`), a comma with nothing after it, a key that is no text.
- A control character (under U+0020) written as itself inside a text.

And among the values JSON has:

- `-0` is not a whole number. Where a whole number is asked (an entry's width, a
  query's place, a setting's value, any count) it is `bad_value`, as `1.0` and `1e2`
  are; it is not taken as 0 and then refused as `not_canonical`. Nor are `true` and
  `false` whole numbers.
- A number with a fraction may be -0 (`"8000000000000000"`) wherever its field takes 0:
  a score, a predicted IoU, an edge of a box, a coordinate of a point. It is read as -0
  and written back with its own bits; 0 and -0 are two writings of two numbers.

### The messages

Six, told apart by `type`. Every message has an `id`: 1 to 64 characters, a lower-case
letter or a digit and then lower-case letters, digits, `.`, `_` and `-`.

**`hello`.** Fields: `type`, `id`, `version`, `role`, and for a worker `device` and
`threads`.

| Field | Value |
|---|---|
| `version` | `pack/1` |
| `role` | `whizbang` or `worker` |
| `device` | Worker only. The device a model's work runs on, as the runtime names it (`cpu`, `mps`, `cuda:0`): a lower-case letter, then lower-case letters, digits, `.`, `_`, `-` and `:`, 32 characters at most. `null` while the worker has not asked its runtime |
| `threads` | Worker only. How many threads the worker's process holds now: 1 to 65,535 |

Whizbang says hello first; the worker answers with a hello of the same `id`. That id
names the conversation: an `error` that carries it is about the conversation and ends it.

**`request`.** Fields: `type`, `id`, `kind`, then the kind's own. The id is one its
sender has not open. Three kinds at `pack/1`, each asked of a worker:

| Kind | Fields after `kind` |
|---|---|
| `load` | `model`, `installation`, `settings` |
| `unload` | `model`, `installation` |
| `segment` | `model`, `installation`, `image`, `queries`, `max_instances` |

- `model` is a model's name as ADR 0017 has one. `installation` is an installation's
  identity: `sha256:` and 64 lower-case hex digits. The worker is told where the models
  directory is when it is started, and finds an installation by those two; a `segment`
  names them too, and is answered only by the model that is resident.
- `settings` is the model's settings, as the config gives them (ADR 0011, "What the file
  holds"): a list of at most 32 `{"name":…,"value":…}`, in the order of the names' bytes,
  no name twice. A name is a lower-case letter, then lower-case letters, digits and `_`,
  64 characters at most. A value is `true`, `false`, a whole number that fits in 64 bits
  with a sign, or a text of 1 to 1,024 printable ASCII characters with no space at
  either end and neither `/` nor `\` in it. What rascal's adapters took for each call (a
  threshold, a padding, an outline rule, a least cover) are settings here: they arrive
  once, in `load`, and are no field of a `segment`. So does `device` (plan 1D, task V1):
  `auto`, where the adapter's runtime places the model on the GPU when it has one, or
  `cpu`, which places it on the CPU whatever the machine has; the result's `device` says
  where it was placed. A value no adapter takes is refused at the `load`, `rejected`.
- `image` is a payload's entry (below) for the picture: the canonical pixels of ADR
  0001, under the binding the request names (ADR 0006, as amended).
- `queries` is 1 to 64 of `{"phrase":text}` (1 to 512 bytes), `{"point":[x,y]}` and
  `{"box":[left,top,right,bottom]}`, each number written as a number with a fraction is.
  A point's numbers are finite and not under zero; a box's too, with its left under its
  right and its top under its bottom. Whether a point or a box is inside its image is
  the capability's rule (ADR 0006), held where a request is made, and is not looked at
  again by the frame's reader.
- `max_instances` is 1 to 64.

A request may be sent in either direction, so that a tool in the worker can ask
whizbang for a capability at milestone 2. At `pack/1` every kind is asked of a worker:
a worker sends none, and a request from one is answered with an `error` whose fault is
`unsupported` (`docs/carry-over.md`).

**`result`.** Fields: `type`, `id`, `kind`, `device`, `threads`, `diagnostics`, and for
a `segment` `instances`. It ends its request, and its `kind` is the request's.

| Field | Value |
|---|---|
| `device` | The device that answered, as in a hello, never `null` |
| `threads` | As in a hello, counted after the call |
| `diagnostics` | A text of the protocol (below), which may be empty |
| `instances` | For `segment`: the instances found, in the fixed order below |

An instance is `{"query":…,"score":…,"iou":…,"box":[…],"plane":…}`:

- `query` is the place in the request's `queries` of the query it answers, from 0.
- `score` is a number with a fraction from 0 to 1. `iou` is the model's predicted IoU,
  0 to 1, or `null` when the model gives none. Whatever else an adapter measured of an
  instance goes into the result's `diagnostics`, in the adapter's words.
- `box` is left, top, right and bottom, four finite numbers with a fraction.
- `plane` is a payload's entry for the instance's plane.

**The fixed order of an answer.** Best score first; among equal scores, by `query`; then
by the box's top, then its left, then its right, then its bottom; then by the predicted
IoU, `null` first and then the lesser first; then by the plane's bytes, the lesser
first. Numbers with a fraction are compared by IEEE 754's total order, which on the
finite numbers a header holds is their value, with -0 less than 0: so among scores,
which go best first, 0 comes before -0, and among edges and predicted IoUs -0 comes
before 0. (It is the order of the 64 bits read as a whole number with a sign, once the
63 lower bits of one under zero are flipped.) The order is total: two instances equal
in every key are equal in every byte a result holds of them, so the order the writer
gives is the only one a reader takes. The predicted IoU comes before the plane's bytes
so that every key but the last is in the header: the reader holds those before a plane
is read, and the header it writes again to check its canonical writing, which has no
planes yet, is in the same order. The writer sorts, whatever order the answer was built
in, and the reader refuses a result whose instances are in another order
(`out_of_order`): by what the header shows before a plane is read, and by the planes
once they are. This is rascal's one order (`order`: score, query, top, left), made
total. Among the instances of one query it is best score first.

**`event`.** Fields: `type`, `id`, `text`. Something a call says before it ends: `id` is
an open request's, and `text` is a text of the protocol, not empty.

**`cancel`.** Fields: `type`, `id`. Asks that the request with this id stop. It is
answered by that request's own end: its result, if it had finished, or an `error`
whose fault is `cancelled`.

**`error`.** Fields: `type`, `id`, `fault`, `sentence`. It ends the request with that
id in place of a result; with the hello's id it ends the conversation. `fault` is one of
ADR 0006's eight names (`cancelled`, `unsupported`, `credential`, `rejected`,
`unavailable`, `timed_out`, `malformed`, `too_large`). `sentence` is a text of the
protocol of 1 to 240 characters.

**A text of the protocol** (a result's `diagnostics`, an event's `text`, an error's
`sentence`) holds only ASCII letters, digits, spaces and `. , ; : ' ( ) % + - _`: the
characters ADR 0006 ("Faults") keeps when it cleans a fault's sentence, without the `?`
that cleaning puts for every other. A reader refuses any other character; nothing is
replaced. So such a text has no `/` and no `\`, no line break, no double quote and no
`?`. A writer that cleans a text before it writes it puts a space for a character the
text may not hold, never a `?`. (Amended 2026-11-17: this said "what a fault's sentence
may hold after cleaning", which holds `?`.)

### A payload's entry

Raw samples, described by an entry of seven fields: `width`, `height`, `layout`,
`data`, `colour`, `alpha`, `binding`.

| Entry | `layout` | `data` | `colour` | `alpha` | Bytes |
|---|---|---|---|---|---|
| A request's `image` | `rgba`: four samples a pixel, red, green, blue, alpha, row by row from the top left | `u8` | `srgb` | `straight` | 4 × width × height |
| An instance's `plane` | `plane`: one sample a pixel, row by row from the top left; 255 on the thing, 0 off it | `u8` | `none` | `none` | width × height |

- `width` and `height` are at least 1, and their product at most 50,000,000 (ADR 0006's
  `PIXEL_LIMIT`).
- `binding` is the identity of the binding the samples belong to (ADR 0001): `sha256:`
  and 64 lower-case hex digits, 71 characters. A plane's entry states the width, the
  height and the binding of its request's image, or its result is refused.
- No other layout, data type, colour rule or alpha rule exists at `pack/1`, and an entry
  with another, or with the picture's rules on a plane, is refused.
- The payload's own length, in the 8 bytes before it, is the entry's count of bytes, or
  the frame is refused before a sample is read.

What the samples of a plane must be (0 or 255, something on, inside its box) is the
capability's rule, held when an instance is made of them (ADR 0006, "Segment").

### Bounds

Every bound is known before the bytes it bounds are read, and a reader sets nothing
aside for a frame it refuses.

| What | Bound | Looked at |
|---|---|---|
| A header | 65,536 bytes (64 KiB) | The 4 bytes of its length, before the header is read, against the largest bound among the requests the reader has open. A header over 64 KiB that is no result's, read while a request allows more, is held to 64 KiB with what is open: after its values and its canonical writing, and for an event once its request is known to be open (amended 2026-11-17) |
| A result's header | 65,536 bytes and 512 for each instance its request allows: `queries` × `max_instances`, so 2,162,688 at ADR 0006's most (4,096 instances). A `load`'s and an `unload`'s allow none: 65,536 | The same 4 bytes, against the largest bound among the requests the reader has open, which is the call's own while one call is in the worker at a time; then against its own request's, once its `id` is read |
| A request's payload | One picture: 4 × width × height, at most 200,000,000 bytes | The header, before the payload |
| A result's instances | For each query, `max_instances` of its request; each plane the size of the request's image | The header, before any payload |
| A result's payloads together | 1,073,741,824 bytes (1 GiB), whatever its request would allow | The header, before any payload: with what is open, after every instance is held to its request (amended 2026-11-17) |
| Settings in a `load` | 32 | The header |
| Events of one request | 256 | Counted by the reader for each open request |
| Diagnostics of one request | 65,536 bytes: the text of every event and the result's `diagnostics`, together | Counted by the reader for each open request |

An instance's entry is at most 330 bytes as written, so the instances a request allows
always fit in the 512 bytes each of them brings. The 64 KiB hold the result's other
fields and its diagnostics, with what the instances leave over. So the header's bound
is a bound of its own: a result whose diagnostics are within about 200 bytes of their
64 KiB, with the longest id and device there can be, is over its header's bound, and
is refused for that.

A writer of a `segment`'s result does not know its request, so it holds the header to
the most any request allows; the reader holds it to the request's.

A reader that asks keeps, for each request it has open, what its answer is held to: the
kind, and for a `segment` the image's width, height and binding, the count of queries
and the most instances; and two counts, of events and of bytes of text. It notes a
request as open before it writes the request's frame. A result or an error closes the
request. The 257th event of a request, and text past the bound, are refused and not
kept; the supervisor then ends the call as malformed and restarts the worker (task P4).

Not counted toward the 64 KiB of diagnostics, because each has a bound of its own: an
instance's score and predicted IoU, typed fields of its entry; the payloads; and the
worker's error output, which the supervisor bounds by lines (task P3a).

### What a refusal says

A frame is refused for one reason of a closed list. A refusal is the reason's name and
a sentence of fixed words and numbers: it quotes nothing the other side sent, but for
the name of another version, and that only when it is a plain name.

| Reason | When |
|---|---|
| `truncated` | The input ended, or failed, inside a frame |
| `header_too_large` | A header's stated length is 0 or over its bound |
| `not_json` | The header is not strict UTF-8, is not one JSON object, or gives a field twice: each case of "What a reader takes as JSON" |
| `unknown_message` | `type` is none of the six |
| `version` | A hello states another version. The sentence names both |
| `unknown_kind` | A request or a result is of a kind `pack/1` has not |
| `unknown_field` | An object holds a field that is not its own |
| `missing_field` | An object lacks a field of its own |
| `bad_value` | A value is of the wrong type, breaks its field's rule, or is not finite |
| `over_bound` | A count or a size is over its bound: settings, queries, the most instances, a phrase, pixels, a query's instances, events, diagnostics. A whole number in a field of a count or a size (a width, a height, a count of threads, a query's place, the most instances) is held first to the 32 bits the field holds, and past them is `bad_value` (amended 2026-11-17) |
| `out_of_order` | Settings are not in the order of their names, or a name is given twice; instances are not in the fixed order |
| `not_canonical` | The header is not the canonical writing of what it holds |
| `unexpected` | A result or an event names a request the reader has not open, or a result is of another kind than its request |
| `wrong_size` | A plane's width and height are not its request's image's |
| `wrong_binding` | A plane names another binding than its request's image is under |
| `too_large` | A result's payloads together are over 1 GiB |
| `wrong_length` | A payload's stated length is not what its entry gives |

The rules are looked at in this order: the header's length; that it is JSON; `type`;
for a hello, `version`, before any other field, since another version may have other
fields, and then `role`, which says which fields the hello has; for a request or a
result, `kind`, which says the same of it; in each object, a field that is not its own,
then one that is missing; the values, and with them the counts and the orders a header
shows; the canonical writing; what the reader has open; and then each payload's length.
`type`, `version`, `role` and `kind` are each `missing_field` when absent and
`bad_value` when they are no text; a `role` that is neither name is `bad_value`, so a
hello with such a role and a field that is no hello's is refused for its role. A frame
that breaks rules of two of these steps is refused for the earlier step's; among the
values of one header, which is named is the reader's own. Each refusal vector breaks one
rule, or, where no frame can break that rule alone (JSON nested too deep, a number out
of range), breaks it at an earlier step than the other. The ten added on 2026-11-17 each
break rules of two steps on purpose, and state the earlier step's reason, as "The steps
in full" orders them.

*Added 2026-11-17, by task P3b (the supervisor), changing no byte and no reason:*

- **`over_bound` of a query's instances** is any `over_bound` of a count of a result's
  instances, for one query (step 7) or in all, past 4,096 (step 5); a refusal says
  whether it is one (`Refused::counts_instances`). Every other `over_bound` (events,
  text, settings, queries, pixels) ends a call as `malformed`.
- **A request whizbang would refuse to write** is no frame and ends no conversation: it
  is a fault of its call before anything is sent, `too_large` for
  `header_too_large`, `over_bound` and `too_large`, and `rejected` for every other
  reason (`proto::writable`).
- **A request from a worker** is refused before a byte of its payloads is read
  (`proto::hear`), so that a worker cannot make whizbang read 200 MB of pixels it will
  not use. It is answered with an `error` of its id whose fault is `unsupported`, as
  "The messages" says; the input is then no longer at the start of a frame, so the
  conversation ends too, and the call that was open is `malformed`.
- **The `error` whizbang sends** after a refusal carries the conversation's id and the
  fault the refusal ends the call as; its sentence is the refusal's, cleaned as a text
  of the protocol ("A text of the protocol": a space for each character it may not
  hold, never a `?`; `proto::clean`, `proto::sentence`), since the refusal of another
  version names both, with their `/`.

#### The steps in full

*Added 2026-11-17. This is what whizbang's reader does, and both readers do it; no byte
or reason changed.* "The values" above is two passes, and "what the reader has open" is
looked at in a fixed order. Among several faults of one pass the reader names the first
it meets in this order; which of two faults in one field's rules is named stays the
reader's own.

1. The header's length, against the largest bound among the requests open (a header of
   no result's is held to 64 KiB at step 6).
2. That it is JSON.
3. `type`; a hello's `version` and `role`, or a request's or a result's `kind`.
4. **Each object taken apart.** In each object a field that is not its own, then one that
   is missing; then its fields, one by one, each of its type, with the rules that are held
   where the field is read:
   - a hello: `id` (a text), `device` (a text or null), `threads`;
   - a request: `id` (a text), `model` and `installation` with their rules, then a load's
     `settings`, or a segment's `image`, `queries` and `max_instances`. Each setting in
     turn: a field that is not its own, one that is missing, its `name` as a text, its
     `value` as a scalar, the rules of the name and of a text, and then its order after
     the setting before it (`out_of_order`). Each query in turn: a field that is no
     query's is `unknown_field`; two of a query's own fields is `bad_value`; none is
     `missing_field`; then a phrase of 1 byte at least (`bad_value`) and 512 at most
     (`over_bound`), or a point's or a box's numbers and their rules (`bad_value`). An
     entry: its fields, its width and height, its four words, its binding;
   - a result: its `instances` first, each with its `query`, `score`, `iou`, `box` and
     `plane` entry; then `id`, `device`, `threads` and `diagnostics`, each as a text or
     a whole number;
   - an event: `id`, `text`; a cancel: `id`; an error: `id`, `fault` (one of the eight),
     `sentence`.

   A whole number in a field of a count or a size is held here to 32 bits (`bad_value`
   past them); a number with a fraction to its 16 hex digits, whatever double they name.
5. **The rules of the values and counts.** The id's rule; a hello's device and threads;
   a load's count of settings (`over_bound` past 32); a segment's image (no pixel is
   `bad_value`, over 50,000,000 is `over_bound`), its count of queries (none is
   `bad_value`, over 64 is `over_bound`) and its most instances (0 is `bad_value`, over
   64 is `over_bound`); a result's device, threads, diagnostics (`over_bound` past
   65,536 bytes) and count of instances (`over_bound` past 4,096), then each instance's
   score and predicted IoU (from 0 to 1, so finite), box (finite) and plane's pixels; an
   event's text (`over_bound` past 65,536 bytes); an error's sentence (`bad_value` past
   240). Then the fixed order of a result's instances, by what the header shows.
6. The canonical writing.
7. **What the reader has open.** A result: that its request is open and of its kind
   (`unexpected`), its header within its request's bound (`header_too_large`), its
   diagnostics within the text left to the request (`over_bound`); then instance by
   instance, its query's place among the request's queries (`bad_value`), the count of
   instances for that query (`over_bound`), its plane's width and height (`wrong_size`)
   and binding (`wrong_binding`); and last, its planes together within 1 GiB
   (`too_large`). An event: that its request is open (`unexpected`), its header within
   64 KiB (`header_too_large`), the count of events and the text (`over_bound`). Any
   other message: its header within 64 KiB (`header_too_large`).
8. Each payload's length, before its samples are read (`wrong_length`).
9. The order of instances the header showed tied, by their planes (`out_of_order`).

After a refusal the reader cannot know where the next frame starts, so the conversation
is over: the side that refused sends an `error` with the conversation's id, when it can
still write, and closes. What each refusal becomes for the call that was open is the
supervisor's (tasks P3b and P4): `header_too_large`, `over_bound` of a query's
instances and `too_large` end it as the fault `too_large`; every other reason as
`malformed`.

### The amendment to ADR 0006

A payload states the binding it belongs to, and ADR 0006's segment request named none.
So, in the core (`capability/segment.rs`; the tests are in
`tests/it/capability/segment.rs`):

- **A `SegmentRequest` carries the identity of the binding its picture is under.** Told
  nothing, that is the picture's own binding: itself in its own space, flat, onto the
  root (`Binding::root`), which is the binding a mask made on the picture names.
  `SegmentRequest::under` replaces it with a binding the caller gives, for a picture
  that is placed onto another image or whose space is not flat. That binding's source is
  the picture, by canonical pixel digest, and its source space is the picture's size, or
  it is refused: the same rule by which a mask belongs to an image (ADR 0002).
- **Two requests of the same pixels under two bindings are two requests,** and are not
  equal.
- **An `Instance` names the binding its plane belongs to** (`Instance::binding`), and is
  not made without one.
- **The handle refuses, as `malformed`, an answer in which any plane names another
  binding than its request's.** So an answer cannot be taken for the answer to another
  request of the same pixels.

The codec writes the request's binding into the entry of its image, and holds each
plane of the result to it (`wrong_binding`) before a plane is read; the handle's check
is the same rule for every backend, the pack or not.

Not changed: what a run records of a request, and the fingerprint of a prepared call.
A segment request is in neither yet.

### Nothing is a path

No message has a field for a path, a folder or an address. The worker is told where the
models directory is when it is started, and a `load` names a model and an installation's
identity, each of which is one plain name or a digest. Every text a peer fills in and
the other side acts on (an id, a model's name, an identity, a device, a setting's name
and its text, a text of the protocol) is held to a rule that leaves no `/` and no `\`
in it. One text may hold any character: a phrase, which is words for the model and is
never opened, joined to a folder or run.

### The HTTP codec, on paper

Decided here and built at milestone 6 (`docs/carry-over.md`). The frames are the same
bytes.

- One request is one HTTP request: a `POST` whose body is whizbang's hello and then the
  `request` frame, with a stated length. The address is the worker's and the path is
  fixed by the version; nothing of a message is in the address or in a header field.
- The response's body is streamed: the worker's hello, then the request's events as
  they happen, then its result or its error, and the body ends. A response that ends
  before a result or an error is `truncated`.
- A `cancel` is a second `POST`, of a hello and the `cancel` frame; closing the first
  connection asks the same.
- Every bound above holds as it does locally, on the bytes of the two bodies. The
  credential, the origin's approval and redirects are the checked sender's (ADRs 0012
  and 0019), not the protocol's.

The behavioural half of the conformance suite (task P3b) is written against a transport,
so that milestone 6 runs it again over this one.

### What `pack/2` would mean

`pack/1` is these bytes. A new field, a new kind of request, a new layout, a changed
bound or a changed meaning is another version with another name, decided by another
record. A side speaks exactly one: whizbang and the pack it installs are built from one
commit, so they agree, and a worker of another version is refused at its hello and is
not left running (task P3a). Nothing reads two versions, and no field says which parts
of a version a side supports.

What may be added to this record without a new version: sections on what the supervisor
does around the frames ("The worker", task P4), since they change no byte.

### The worker

Added by task P4, and amended by task P5b; no byte of `pack/1` changed. The code is
`crates/whizbang-backends/src/pack/queue.rs` and `worker.rs`; the tests are in
`crates/whizbang-backends/tests/it/pack/queue.rs` and, for removal, `removal.rs`. All of
it is on the supervisor's injected clock.

**The queue.** One supervisor in a process (plan 1C, D4) holds the one worker and a
queue in front of it. A request takes a number and a place at the back; at most **16**
wait, and the 17th is refused at once, `unavailable`, `not_sent`, "the queue of the
pack's worker is full, 16 requests wait; this one was not held", and is not held. A
request's turn is the front of the queue with no turn holding the worker: one call is
in the worker at a time, in the order the requests came. The turn runs on its caller's
own thread, so the queue makes no thread; each worker still has the supervisor's three
(plan 1C, "Rules", "Threads in whizbang"). A request keeps the count of removals its
hold was given while it waits (D5); task P5b compares it with the model's ("Removal",
below). A waiting request sleeps until the earliest of its deadline, its longest wait
and its next read of that count, or until the queue changes (task P5b, from the review
of task P4: until then every waiting request woke every 5 ms); so its cancel is seen
within 250 ms.

**A turn.** The worker holds one resident model: its name, installation and settings.
A turn whose own are not resident first sends an `unload` of the resident one, then a
`load` of its own, then its call; one that finds its own resident sends its call alone.
So two requests for one model cause one load, and a request for another model waits for
the current call and then unloads the first. Each of the three is one request of
`pack/1`, within its kind's limit: 30 s for an `unload`, 120 s for a `load`, 120 s for a
`segment` (D7).

**What a caller gives with a request** (`Limits`; amended by task P6a, which hands them
from the kind `pack`): its cancellation and its deadline, which are the run's; and two
of the kind's settings (ADR 0011, "The kind `pack`"). `call_limit` is the limit of a
`segment` when it is shorter than 120 s; a longer one is 120 s, so a turn stays within
the 365 s below. `lifetime_s` is the `--max-lifetime-s` of a worker started for the
request; a worker already running keeps its own. No byte of `pack/1` changed.

**The states of a request, and what each fault says.**

| Where it is | What ends it | Fault, stage | Sentence |
|---|---|---|---|
| Asked, the queue full | At once | `unavailable`, `not_sent` | the queue of the pack's worker is full, 16 requests wait; this one was not held |
| Waiting | Its cancel | `cancelled`, `not_sent` | the request was cancelled while it waited in the queue of the pack's worker; it was not sent |
| Waiting | Its deadline | `timed_out`, `not_sent` | the request's deadline passed while it waited in the queue of the pack's worker; it was not sent |
| Waiting, with no deadline | 5,840 s of the clock: the turns of every request that can be ahead of it, each at most 365 s (a hello, an unload, a load, a call, a cancel's limit, and six of 5 s) | `timed_out`, `not_sent` | the request waited 5840 seconds in the queue … the most the calls ahead of it can take; it was not sent |
| Waiting | A restart | `unavailable`, `not_sent` | the pack's worker was restarted before this request's turn came; it was not sent |
| Waiting | A removal of its model in this process, which tells the supervisor when it sets its mark (task P5b) | `rejected`, `not_sent` | the model is being removed; the request was ended while it waited in the queue of the pack's worker, and was not sent |
| Waiting, or taken off the queue | Its model's count of removals, read every 250 ms of the clock and when it is taken off the queue, has moved from the one its hold was given (task P5b) | `rejected`, `not_sent` | a removal of the model began after the request's hold was given; the request was ended while it waited in the queue of the pack's worker, and was not sent |
| Waiting, or taken off the queue | That count cannot be read as one | `rejected`, `not_sent` | the model's count of removals cannot be read; the request was ended … |
| Waiting, or taken off the queue | The count as its own, and the model draining: a mark under a live remover, or what is no mark there under one (`models::draining`, D6) | `rejected`, `not_sent` | the model is being removed; the request was ended … |
| Waiting, or asked | The supervisor ended (`Supervisor::kill`, task P5b) | `unavailable`, `not_sent` | the supervisor of the pack's worker was ended before this request's turn came; it was not sent |
| In its turn, before a byte is written | Its cancel or deadline (looked at before the worker is started, and again after its hello) | `cancelled` or `timed_out`, `not_sent` | … before the pack's worker was asked, or while the pack's worker was started; it was not sent |
| In its turn, the worker not started again | Three workers in a row that ended before they answered anything (below) | `unavailable`, `not_sent` | the pack's worker ended before it answered anything 3 times in a row; no worker is started until 60 seconds after the last |
| In the worker | Its result, or the worker's `error` for it | none, or the worker's fault, `answered` | the pack's worker ended the request with the fault … |
| In the worker | Its kind's limit | `timed_out`; the worker killed | the pack's worker did not end the request within … seconds and was killed |
| In the worker | Its deadline, when earlier | `timed_out`; the worker killed | the request's deadline passed before the pack's worker ended it, and the worker was killed |
| In the worker | Its cancel, then 5 s | `cancelled`; the worker killed | the pack's worker did not stop the cancelled request within 5 seconds of its cancel and was killed |
| In the worker | Its cancel, then its deadline or its kind's limit, when earlier | `cancelled`; the worker killed | the pack's worker had not stopped the cancelled request by the end of its wait and was killed |
| In the worker | The worker ends, breaks the protocol, or cannot be written to | as task P3b made them (`unavailable`, `malformed`, `too_large`) | as task P3b made them |
| In the worker, or being started | The supervisor ended (`Supervisor::kill`, task P5b): at the call's next look, without waiting for the call | `unavailable`; the worker killed | the pack's worker was killed when its supervisor was ended (… while it was started, when its supervisor was ended) |

The row of a cancel and then a deadline is
`queue::a_cancelled_call_whose_deadline_comes_before_five_seconds_after_its_cancel_is_killed_at_its_deadline_as_cancelled`
(task P5b, from the review of task P4).

A request in the worker that ended it is `unresolved` once a byte of it left, and
`not_sent` when none did (task P3b). Each sentence of one that ended the worker then
says what the next request finds (changed by task P5b, from the review of task P4: until
then every such sentence ended "; the pack's worker is restarted for the next request",
which was false after the third failure in a row and at a start that failed):

- "; the pack's worker is restarted for the next request", of a worker that said its
  hello, when the next start does not wait;
- "; it said no hello, and the next request starts another worker", of a worker that
  ended before its hello, when the next start does not wait;
- "; 3 workers in a row ended before they answered anything, and no worker is started
  until 60 seconds after the last", when it does
  (`queue::a_fault_that_ended_the_worker_at_its_start_says_what_the_next_request_finds`);
- nothing more, when the supervisor was ended.

**Cancellation is cooperative.** A cancel is not a flag the supervisor keeps: it is the
caller's signal (`Cancellation`, ADR 0008), looked at between two looks at the worker's
output. In the worker, the first look that finds it set hands `cancel` with the
request's id to the writer, waiting for nothing, and the call then waits at most 5 s
more. The worker answers it with an `error` whose fault is `cancelled`, or with the
result it had finished, and lives on; one that does neither in time is killed.

**A deadline** is the caller's, an instant of the supervisor's clock. A call in the
worker waits until the earliest of its kind's limit, its deadline and, once cancelled,
5 s after its cancel; past it the worker is killed and waited for. The call is ended
there, at the worker's boundary, whether or not the worker would have stopped.

**A restart** is the end of the worker inside a turn, by any cause: killed at a limit,
a deadline or an ignored cancel; ended by itself; ended after a refused frame. It ends
the request whose turn it was, and every request waiting, as the table says. Nothing
that was sent is sent again, and nothing waiting is sent to any worker. The supervisor
starts no worker by itself: the next request starts one (D7). **No loop of starts.** A
worker that ends before it answered anything (by a result or an `error` for a request),
at no caller's deadline or cancel, counts; one that answers sets the count to none.
After 3 in a row, no worker is started until 60 s of the clock after the last: a request
in that time is refused at once, as the table says; one after it starts one worker, and
if that one ends the same way the pause begins again. So a worker that dies at every
start is started at most once a minute, by a request, after the first three.

**Removal** (task P5b; plan 1C, D5 and D6; ADR 0017, "Removal"). The supervisor is
made with the models directory the worker is told of, and reads counts of removals
there. Two ways reach the queue, and neither can be missed:

- **A removal in this process** is made by `models::remove_and_tell`, which tells its
  caller each time it sets its mark, with the count raised; the caller tells the
  supervisor (`Supervisor::removing`). Every request waiting for the model ends at once,
  `rejected` and `not_sent`, and its caller lets its hold go; when the model is resident
  and no turn holds the worker, a turn of the supervisor's own unloads it, on the
  remover's thread, within the limit of an unload, and the journal numbers that unload
  0 (`removal::a_removal_refuses_new_requests_cancels_the_queued_ones_and_waits_for_the_current_call`,
  `removal::a_removal_in_this_process_unloads_the_model_that_was_resident_and_idle`).
- **A removal in another process** tells nothing. Each waiting request reads its
  model's count every 250 ms of the clock, and again when it is taken off the queue,
  without the queue's lock; a count that has moved from its own, or cannot be read,
  ends it, `rejected` and `not_sent`, whether or not the mark is still there. With the
  count as its own, it asks `models::draining`, the one question of a mark (D6), and a
  yes or a refusal ends it the same way: only a mark set by hand, under a remover's lock
  held by hand, moves no count
  (`removal::a_removal_from_a_second_process_cancels_the_queued_requests_within_the_interval_while_another_call_holds_the_worker`,
  `removal::a_removal_that_began_and_timed_out_between_two_looks_still_cancels_the_requests_queued_before_it`).

New requests are refused by the lease, whose hold the mark refuses (1B). The current
call is not ended: the removal waits for its hold, which is let go when the call is
over. A removal that times out takes its mark off, and the model takes requests again:
a hold given then has the raised count, and its request is not ended
(`removal::a_removal_whose_wait_times_out_answers_busy_and_the_model_takes_requests_again`).
The mark of a remover that died stops nothing here either
(`removal::after_a_remover_was_killed_with_its_mark_set_a_request_for_the_model_is_answered_by_the_worker`,
with a real process that is killed). One limit, stated by D5: an idle worker in another
process keeps a removed model in memory until its next request or its end; no request
is answered from it, since the lease refuses every one.

**Its end** (`Supervisor::kill`; task P5b, from the review of task P4, which found that
the test kit's guard waited for a call in the worker that would not end, so that a test
that failed hung until the runner's limit). Every request waiting ends, as the table
says; the call in the worker ends at its next look (a pause of 5 ms of the clock apart),
and the worker is killed and waited for; a worker being started is ended the same way;
and nothing is taken after, a request asked then refused at once and no worker started
(`queue::the_guard_ends_the_call_in_its_worker_at_once_and_every_waiting_request`, with
a call to `fake-deaf` and a request waiting behind it).

**What the supervisor tells.** Its status (who waits, with each one's number, model and
count of removals; the call in the worker, its kind, its model, how many events it has
said, whether it was written whole and whether its `cancel` was sent; the resident
model; the worker's process id), a wait for a state of it, bounded on the clock, and a
journal of the last 64 requests written to a worker, each with its request's number and
the worker's process id. They are diagnostics, and what the tests of this section read.
A call is told as written once the worker speaks of it in an event, which can come
before the writer says it wrote it (task P5b: until then a status could show an event of
a call whose worker it did not name yet, and P4's test of a slow load read it so).

### The adapter of an installation

Added by task U1 of plan 1D; no byte of `pack/1` changed and its vectors are as they
were. A `load` keeps its fields. The worker, which knows the models directory it was
started with, reads the record of the installation the `load` names
(`models/<model>/installations/<hex>/RECORD`, through a name that is no link, at most
32 KiB), works out its identity again (ADR 0017, "The identity of an installation") and
refuses (`rejected`) a record whose identity is not the one named; then it takes the
adapter the record names, not the one of the model's name. For `user-<name>` the
adapter is the copy `adapters/<revision>.py`, read once, its SHA-256 compared with the
record's revision before anything of it runs, and the bytes that were hashed compiled; a
copy that is missing, a link, too long or of another hash is `rejected`, and a model of
the shipped catalogue is never run by one. An installation with no record (the pack's
own tests, which install nothing) is answered by the adapter of the model's name, as
before (`test_worker.Adapters.…`).

## Consequences

- What is read from a worker is bounded before it is built: a header by its four bytes,
  a result by its request, and one result by 1 GiB. The row of the carry-over list on
  what is read off a wire is closed for the codec; the supervisor uses it from task P3b.
- The settings rascal's adapters took are a model's, sent once in `load`. An adapter
  reads them from what `load` gave it. A setting with a fraction is written in the
  config as a text, as ADR 0011 already asks, and the adapter reads the number.
- A request's header is held to 64 KiB like every header but a result's. The most
  phrases of the most bytes, 64 of 512, fit in about half of it. A batch whose phrases
  are full of characters that are written as two or as six (a double quote, a control
  character) can be over, and is refused where it is written (`header_too_large`); the
  core's own bounds allow it.
- A setting's text cannot hold `/` or `\`, which the config allows. The kind `pack`
  (task P6a) refuses such a setting where the config is read, or its `load` would be
  refused when it is written.
- An answer from the pack has one order. The core's interface still leaves the order to
  the backend (ADR 0006): the pack is one backend, and its order is this one.
- An instance's predicted IoU travels with it and its other diagnostics travel as text.
  What a run keeps of either is the supervisor's (task P3b); the core's `Instance` holds
  neither. (Decided by task P3b: the core's `Instance` and response stay as they are;
  the supervisor hands both on with each call, and the worker's error output with each
  failure; a run keeps them once a backend can hand diagnostics to its run, which none
  can yet: `docs/carry-over.md`, with the `segment` action at 1D.)
- A header's numbers with a fraction cannot be read by eye. The vectors state each
  message a second time in plain numbers.
- Reversing this costs both codecs, the vectors and every later task of the strand. It
  is cheap now: nothing but this codec speaks it.

## Alternatives

- **MCP, or JSON-RPC a line at a time,** as the old tools spoke. Turned down by
  `SPEC.md` section 17 and the plan's D1. A line cannot be bounded before it is read
  without a ceiling for every message, and a plane inside JSON is base64: a third more
  bytes, and a second copy to decode.
- **A binary header.** Turned down: a JSON header is read by the standard library of
  both sides and by a person; its cost is the canonical writing, which the vectors hold.
- **JSON numbers for scores and coordinates.** Turned down: the same double is printed
  as `1e-07` by one library and `1e-7` by another, so no vector could be written back
  byte for byte by both, and a reader that accepts every spelling has no canonical
  writing to hold a header to.
- **One length for all payloads, or none** (the entry's dimensions alone). Turned down:
  the plan asks that each payload carry its length, and a length that must agree with
  its entry catches a writer whose two halves disagree before a byte is taken for
  samples.
- **A result's header bounded at 64 KiB like every other,** with instances in a payload
  of their own. Turned down by the plan: the header names every instance, and its bound
  is worked out from the request.
- **Settings in each `segment`,** as rascal's adapters took them. Turned down by the
  plan and by ADR 0006: they are an adapter's, not a call's.
- **Leave the order to each adapter.** Turned down: rascal sorted every adapter's answer
  one way, and a test of a real model would otherwise depend on the order its runtime
  happens to give.
- **Negotiate a version, or a list of features.** Turned down by the plan: both sides
  are built from one commit.
- **Clean a text of the protocol in place of refusing it.** Turned down: a reader that
  repairs what it reads cannot write it back byte for byte, and a worker that sends a
  path in an event is wrong, not untidy.
