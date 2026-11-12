# The baseline package

The baseline is what the old squint harness did on 2026-11-10 and 2026-11-11, kept so that
milestone 2 can prove whizbang reproduces it (SPEC section 16) and so the study can be written
up from the records alone.

The package is owner-only. It holds evaluation items, their labels and every recorded model
exchange, so it is never published. This folder holds what can be public: `manifest.toml`.

## What the package holds

- **Replay fixtures** for the arm the old harness called C (the `view` and `align` tools).
  For each decision: the attempt its result names, with the journal, every stored exchange
  and tool artifact, and the prepared images. Attempts the old run abandoned are kept apart.
- **The input closure:** the set with its seal, the matrices and configs, the prompt, the
  provider catalogue, the rules, the tool definitions, the pack's lock file, the dependency
  lock files and the preparation code.
- **Study inputs** for every arm and repetition: each decision's result and report without
  image payloads, each run's descriptor, summary, metrics and spend ledger, the labels, the
  corrections with their version, the family assignments, the scoring policy, and the
  bootstrap seed, resample count and correction.
- **Three provenance records:** the harness each run was executed with, the squint commit the
  package was assembled from, and the commit used for any recapture.
- **A private inventory** with one line per input and the kind of evidence that ties it to
  the historical runs.

## How an input is tied to the runs

| Evidence | Used for | What was checked |
|---|---|---|
| Recorded digest | Matrices, configs, prompt, catalogue, set components, tool definitions, pack lock, stored exchanges and artifacts | The file is hashed the way the old harness hashed it and must equal what the run's descriptor or journal recorded. A mismatch blocks the package. |
| From a revision | Dependency lock files, code the run did not hash, the rules layers the configs name | The full commit and path are named, the bytes are kept, and the hash is marked as taken at assembly. |
| Preparation | Prepared images | The recorded preparation version and the prepared images' hashes are matched to the historical config, catalogue and code. |

Files gathered at assembly time are stored apart from the ones a recorded digest covers.

## The manifest

`manifest.toml` holds only:

- the package version;
- one digest over the package;
- per model and arm, the decision count and the spend;
- the three provenance records.

It holds no item names, file names or verdicts.

Spend is what each run's ledger settled, so it includes attempts the old run abandoned. Counts
include every repetition.

## The package digest

Computed the same way every time:

1. List every file in the package by its path relative to the package root, with `/` as the
   separator. Leave out the assembly scripts and the two files that hold this listing and
   its digest.
2. Sort the paths by their bytes.
3. Write one line per file: the file's SHA-256 in lowercase hex, two spaces, the path, a
   newline.
4. The digest is the SHA-256 of that text.

A package whose contents change gets a new `package_version` and a new digest.

## The gate receipt

Milestone 2 writes one receipt when it replays the fixtures. It holds exactly:

| Field | Meaning |
|---|---|
| `package_digest` | The digest above, of the package that was replayed |
| `comparison_version` | The version of the comparison code that judged the replay |
| `whizbang_commit` | The full whizbang commit that was replayed |
| `result` | `pass` or `fail` |
