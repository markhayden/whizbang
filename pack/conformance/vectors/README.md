# The fixed vectors of the pack protocol

The fixed half of the pack protocol's conformance suite (`docs/adr/0020-the-pack-protocol.md`):
frames of `pack/1` as bytes, each with what it parses to or why it is refused. The Rust
codec reads them (`crates/whizbang-backends/tests/it/pack/proto.rs`) and so does the
pack's own, so the two cannot drift.

Each vector is one JSON file, named as its `name`:

| Field | Holds |
|---|---|
| `name` | The file's name without `.json`. A refusal's starts with `refused-` |
| `says` | What the vector is, in a sentence |
| `frame` | The frame's bytes, as lower-case hex |
| `open` | The requests its reader has open when it reads the frame, each with its `id`, its `kind`, the count of `events` and the bytes of `text` it was already sent, and for a `segment` the image's `width`, `height` and `binding`, the count of `queries` and `max_instances` |
| `message` | For a frame that is read: the message, with each number with a fraction as a number, and each payload by its `width`, `height`, `binding`, count of `bytes` and the `sha256` of its samples. The message written again is the frame, byte for byte |
| `refused` | For a frame that is refused: the reason's name, one of ADR 0020's list |

A vector has `message` or `refused`, never both. Each refusal breaks one rule, or breaks
it at an earlier step than the other it cannot help breaking (ADR 0020, "What a refusal
says").

## The shape of `message`

`message` is one JSON object with the fields the header holds, by the same names (a
reader compares values, not bytes: the bytes are `frame`'s). What differs from the
header:

| In the header | In `message` |
|---|---|
| A number with a fraction, as 16 hex digits | A JSON number, the same double: `"3fe0000000000000"` is `0.5`. A -0 is `-0.0`, which compares equal to `0.0` as a value; its bits are held by `frame` |
| A load's `settings`, a list of `{"name":…,"value":…}` | One object keyed by the names, each name's value as it is: `{"outline":"whole","pad":-5,"strict":true}` |
| A request's `image`, and an instance's `plane`: an entry of seven fields | Five fields: `width`, `height`, `binding`, `bytes` (the payload's count of bytes) and `sha256` (64 lower-case hex digits of the SHA-256 of its samples, with no prefix). The entry's `layout`, `data`, `colour` and `alpha` are left out: each has one value where it is |

Every other field is as the header has it: `type`, `id`, `version` and `role` of a
hello, and a worker's `device` (a text or `null`) and `threads`; `kind` of a request and
of a result; `model` and `installation`; `queries` as a list of `{"phrase":…}`,
`{"point":[x,y]}` and `{"box":[left,top,right,bottom]}`; `max_instances`; `device`,
`threads` and `diagnostics` of a result, and for a segment its `instances` in the fixed
order, each `{"query":…,"score":…,"iou":…,"box":[…],"plane":…}` with `iou` a number or
`null`; `text` of an event; `fault` and `sentence` of an error.

The vectors were written once, with task P1 of checkpoint 1C: the frames that are read
by the Rust codec, and the refusals from those by changing bytes, and before `pack/1`
was merged those for a total order and for what a strict reader of JSON refuses. Ten
were added with task P2, for frames that break rules of two steps, where the two codecs
once named different reasons: each states the reason the Rust codec gives, and the
order of the steps is ADR 0020's, "The steps in full". They are fixed:
`pack/1` does not change, so a vector is added and never written again.
