# 0023. One serde_json: the tests build the JSON library the binary ships

Status: accepted
Date: 2026-11-16
Spec: section 3, "Crates and their edges"; section 14, the fast tier

## Context

Cargo turns on, for every crate of one build, each feature that any crate of that build
asks for. `just check`, clippy and CI's tests build the whole workspace; `just smoke`
builds the binary alone (`cargo build --release -p whizbang`), with the crates it depends
on and no others.

`whizbang-eval` turned on `serde_json`'s `arbitrary_precision`, and nothing depends on
`whizbang-eval`. So every crate's tests were built with the feature, and the binary
without it. Under the feature a number reaches a `deserialize_any` as a map rather than
as a number, and `serde_json::Value` keeps a number's text as written: `1e400` is read,
`-0` stays `-0`, and `1e2` and `100.0` are two values. Three places of the binary read
JSON in a way that feature changes, and their tests saw the other build: the models
directory's strict reader (`whizbang-backends`, `models/json.rs`), which refuses `1e400`
as no JSON in the binary and read it as an object in the tests; a judge's tool calls
and tool schemas (`whizbang-core`, `capability/judge.rs`), whose size as written and
whose equality follow how a number is written again; and the look at a judge document's
kind (`capability/written.rs`).

`whizbang-eval` needed the feature for one thing: the baseline comparison compares a
number by the text it was written with (`docs/baseline/transcript.md`, "Numbers keep
their text").

## Decision

- No crate turns on a feature of `serde_json` for itself. Its features are set once, in
  the workspace's `Cargo.toml`, and are today `float_roundtrip` alone.
- The baseline comparison reads a transcript with a reader of its own
  (`whizbang-eval`, `json.rs`), which keeps a number's text, marks an exponent as
  `serde_json` did under the feature, and reads exactly what `serde_json` reads, to the
  same depth; it hands each text to `serde_json` to decode. `compare` takes the two
  transcripts as text, and refuses one that is not JSON (`Refusal::NotJson`). What
  counts as a difference is unchanged, so the comparison's version stays `3`.
- `scripts/check-features.sh`, run by `just graph` and so by `just check`, fails when
  `serde_json`'s features for the binary (`cargo tree -p whizbang`, without
  development dependencies) differ from the workspace's with its tests. A test in each
  crate that reads JSON where the feature matters asserts what the binary does with
  `-0`, `1.5`, `1e2` and `1e400`.

## Consequences

- The tests of the three places above test the binary's `serde_json`. Nothing in them
  changes: under the one build each already did what its record says.
- The baseline comparison holds about 200 lines of reader that `serde_json` would
  otherwise give, and a test holds it to `serde_json` on what is JSON and what is not,
  depth included.
- Turning a feature of `serde_json` on later is a change to the workspace's line, seen by
  every crate and by the binary at once, and the check holds the two together. The same
  is not checked for any other crate's features.

## Alternatives

- **The feature for the whole workspace and the binary.** Every parsed `Value` would keep
  number text, an object whose only key is `serde_json`'s private number key would read
  as a number, and serde's buffered paths (untagged and internally tagged enums,
  `flatten`) misread numbers under it. Each reader would have to be checked against it,
  for one comparison's sake.
- **`whizbang-eval` out of the default members, with a check of its own.** The comparison
  is part of milestone 2's gate, which the binary will run: once the binary depends on it
  the feature reaches the binary anyway.
- **`serde_json`'s `raw_value`, to get each number's text.** Another feature to turn on
  everywhere, for what a short reader does.
