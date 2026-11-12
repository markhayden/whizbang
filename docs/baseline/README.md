# The baseline package

The baseline is what the old squint harness did on 2026-11-10 and 2026-11-11, kept so that
milestone 2 can prove whizbang reproduces it (SPEC section 16) and so the study can be written
up from the records alone.

The package is owner-only. It holds evaluation items, their labels and every recorded model
exchange, so it is never published. This folder holds what can be public: `manifest.toml`.

## What the package holds

- **Replay fixtures** for the arm the old harness called C (the `view` and `align` tools).
  For each decision: the attempt its result names, with the journal, every stored exchange
  and tool artifact, and the prepared images. The tool definitions as that arm projected
  them are inside the stored requests. Attempts the old run abandoned are kept apart.
- **The input closure:** the set with its seal and labels, the matrices and configs, the
  prompt, the provider catalogue, the rules, the tools' manifests, the pack's lock file, the
  dependency lock files and the preparation code.
- **Study inputs** for every arm and repetition: each decision's result and report without
  image payloads, and each run's descriptor, summary, metrics and spend ledger.
- **Material gathered at assembly:** the owner's corrections with their version, the item
  kinds, the list of items an agent has seen, the installed pack's files, the three
  provenance records, and two generated files. One holds the analysis settings: scoring
  policy, bootstrap seed, resample count and correction. The other holds the preparation
  settings and the result of the preparation check.
- **A private inventory** with one line per input, or per folder of like inputs, and the
  kind of evidence that ties it to the historical runs.

## How an input is tied to the runs

Every inventory line carries one of four kinds of evidence.

| Evidence | Used for | What was checked |
|---|---|---|
| Recorded digest | Matrices, configs, prompt, catalogue, set components, labels, tools' manifests, pack lock, descriptors, stored exchanges and artifacts | The file is hashed the way the old harness hashed it and must equal what the run's descriptor, seal or journal recorded. A mismatch blocks the package. |
| From a revision | Dependency lock files, code the run did not hash, the rules layers the configs name | The line names the full commit and the path. The bytes are kept under that commit's name, and the hash is marked as taken at assembly. |
| Preparation | Prepared images | See below. |
| Gathered at assembly | Owner files that are not under version control, the installed pack's files, generated files, the provenance records | Nothing historical names these files. The hash is taken at assembly and the line says so. |

Material gathered at assembly is stored in its own folder, apart from the files a recorded
digest covers and apart from the files kept from a revision.

The tools' manifests were not stored by the old runs. They were printed again by the frozen
build with the installed pack, and each one's hash equals the recorded digest.

The list of items an agent has seen is a living owner record, not an input of any run. The
package holds the version current at assembly.

## The preparation check

The old recorder wrote a preparation version and the hash of each prepared image, not the
settings. So the settings were derived as values from the historical config, the catalogue
and the code at the commit the runs were executed with: per model, the long edge, the
resize filter, the format and its quality, and the decode bounds.

Then every image file of the set was prepared again with that commit's code, with no model
call, once per model. Each result's hash was compared with the hash every arm C journal
recorded for that image and model. All 167 images reproduced byte for byte for each of the
seven models: 1,169 of 1,169.

Every image in this set is already within each model's long edge, so the check exercised
decoding and encoding. It did not exercise resizing.

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

Computed the same way every time, from the package root:

1. Take every regular file under the root, at any depth. Leave out exactly three things: the
   top-level `tools/` folder and everything in it, the top-level file `PACKAGE.sha256`, and
   the top-level file `PACKAGE.digest`. The inventory, `inventory.tsv`, is inside the digest.
2. Folders are not listed. A symlink anywhere in the package is an error: assembly stops.
3. Name each file by its path relative to the root, with `/` as the separator and no
   leading `./`.
4. Sort the paths by their UTF-8 bytes, ascending.
5. Write one line per file: the SHA-256 of the file's bytes as 64 lowercase hex characters,
   then two spaces, then the path, then one line feed. The listing is UTF-8 with no byte
   order mark and ends with that last line feed. This listing is what `PACKAGE.sha256`
   holds.
6. The digest is the SHA-256 of the listing's bytes, in lowercase hex.

In `manifest.toml` and in the gate receipt the digest is written with the prefix `sha256:`.
`PACKAGE.digest` holds the bare hex and a line feed.

Once a manifest is merged, any change to the package gets a new `package_version` and a
new digest.

## The gate receipt

Milestone 2 writes one receipt when it replays the fixtures. It holds exactly:

| Field | Meaning |
|---|---|
| `package_digest` | The digest above with its `sha256:` prefix, of the package that was replayed |
| `comparison_version` | The version of the comparison code that judged the replay |
| `whizbang_commit` | The full whizbang commit that was replayed |
| `result` | `pass` or `fail` |
