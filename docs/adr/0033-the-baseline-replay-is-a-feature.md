# 0033. The baseline replay is a feature the shipped binary leaves out

Status: accepted.
Date: 2026-11-19
Spec: sections 13 and 16; plan m2c (`docs/plans/m2c-milestone-2.md`), task E2, item 7;
`docs/baseline/transcript.md`, "The replay"

## Context

The baseline gate (`whizbang eval baseline`) replays every recorded decision and must judge
each submission as squint judged it, refusals word for word. Squint checked a submission
against its JSON Schema with the `jsonschema` crate, and its refusals are that crate's
messages; the replay reproduces them by running the same crate at the version squint's lock
file pins (`=0.60.2`). That crate brings 88 crates into the build. Only the gate needs them;
no user of the shipped binary does.

## Decision

The owner decided on 2026-11-19, asked directly: those crates stay out of the shipped binary.

- A cargo feature, `baseline-replay`, off by default, on `whizbang-squint` (where the replay
  and its validator live: `replay`, and `jsonschema` as an optional dependency) and on
  `whizbang` (where `eval baseline` and its reading of a package live: `baseline`).
- `cargo build --release -p whizbang` links none of them. `scripts/check-features.sh` holds
  that, beside the rule on `fakes`: the binary's own dependency list names no `jsonschema`.
- Without the feature, `whizbang eval baseline` exists, says in one sentence that the build
  was made without the replay and how to build it, and exits 1.
- The tests build with it, by each crate's development dependency on itself, so the captured
  decisions run in every `just check` and on CI. The owner's run builds with
  `cargo build --release -p whizbang --features baseline-replay`.

## Consequences

The release binary does not carry the gate. A reviewer of what ships reads the default
build; the feature's code is read as test tooling. A check of the owner's package needs the
build line above, which `docs/baseline/README.md` and the note of milestone 2 give.

## Alternatives

- **The crate in the binary.** Simplest, and exact; the owner turned it down: 88 crates for a
  check no user runs.
- **Squint's schema messages written by hand.** No crate, but every message of every keyword
  in the crate's order and words would be a guess held by a few vectors; a real run would
  find the ones nobody captured.
- **The gate as a separate binary.** It would need its own crate, packaging and checks, for
  what one feature does.
