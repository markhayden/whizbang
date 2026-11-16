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

A vector has `message` or `refused`, never both. Each refusal breaks one rule.

The vectors were written once, with task P1 of checkpoint 1C: the frames that are read
by the Rust codec, and the refusals from those by changing bytes. They are fixed:
`pack/1` does not change, so a vector is added and never written again.
