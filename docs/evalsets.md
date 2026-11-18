# Evaluation sets

An evaluation item is a complete job and a label. The job is a squint job file that
`whizbang squint check --job` takes as it is; the labels live in a file of their own, so a
blind set ships without them. The sets under `evalsets/` are squint's, copied byte for byte
from squint at the commit `docs/sources.toml` pins (plan 2C, task E1), and held to that by a
test against the list of their digests (`crates/whizbang-eval/tests/squint-evalsets.sha256`).
`evalsets/README.md` is squint's table of every published set: how each was built, labelled
and reviewed. This page is how the sets are laid out and handled here; ported from squint's
`docs/evalsets.md`.

## Layout

```
evalsets/<use-case>/<set>/        the set's id is <use-case>/<set>
  set.toml                        [set] and [[item]] entries (id, family, job)
  families.toml                   [[family]] id and description
  labels.toml                     [[label]] per item; none in a blind set, whose manifest
                                  carries labels_sha256 instead
  seal.toml                       the seal squint wrote: a SHA-256 for each component
  corrections.toml                optional: [[correction]] per label the owner corrected
                                  after the set was scored; outside the seal
  items/<item>/job.toml           a whole job; its pictures named relative to the set's
                                  folder, each pinned by its SHA-256
evalsets/matrix/                  squint's matrices and configs, kept as squint had them
evalsets/receipts/                squint's gate receipts
matrices/                         the matrices whizbang runs (below)
```

A set's folder holds no link: every file of a set, and every picture under `--from`, is
read through none, and one found is refused. No picture is in this repository. Each is fetched by its pin into a store, as a picture
(`whizbang eval sets fetch`).

Every file is read strictly: an unknown key anywhere (an `expected` on an item, a label in a
job) is refused, which is how a manifest or a job is kept free of an answer.

`[set]`: `id`, `use_case`, `split` (`dev`, `heldout`, `blind`), `licence`, `images_repo`
(`github.com/<owner>/<repo>`), `images_rev` (the revision the pictures are at), `labels` or
`labels_sha256`, `families`, and `rules = { base, version }`, the rule file of `check` the
items are judged under.

## Commands

- `whizbang eval sets check [--root evalsets]` reads every set under the root and fails on the
  first problem: a set not at its id's path; a rules base `check` does not ship, of another
  version or use case; an item twice, of an undeclared family, of another use case, or whose
  job `check` does not take under its use case; a picture without a pin; labels wrong for the
  split (a dev or heldout set names a labels file that labels every item once and nothing
  else; a blind set has none and carries `labels_sha256`); a region of the wrong shape; a
  correction that does not say what it corrects; a seal that does not match; and, across the
  sets of one use case, a family in more than one split. It runs offline: the seal's
  component of the pictures is made from the pins, which is what squint made it from wherever
  the pictures matched their pins.
- `whizbang eval sets fetch --set <id> --store <dir> --config <file>` brings each picture of
  the set that the store does not hold into it, as a picture, from
  `https://raw.githubusercontent.com/markhayden/wafflery-evalsets/<images_rev>/<set id>/<path>`;
  a set that names another repository is not fetched. The config
  must approve that origin as a recipient (`[egress.recipients]`), or nothing is fetched; the
  request carries nothing but its address. What arrives is imported only when its SHA-256 is
  the job's pin. With `--from <dir>`, the pictures are read from a folder laid out as that
  repository is (`<dir>/<set id>/<path>`), by the same pins, and nothing is fetched.
- `whizbang eval matrix --matrix <file> --store <dir> --config <file>` runs a matrix (below).

## Labels and corrections

A label found wrong after a set was scored is not edited in place: `labels.toml` stays as
sealed, because the seal and any gate receipt bind its digest. The owner records the fix in
`corrections.toml` beside it, one `[[correction]]` per item, with the label's fields, the
`expected` it replaces (`was`) and how it was found (`found_by`). Reading a set applies each
correction to its item's label; a correction of an item that has no label, a second
correction of one item, or one whose `was` is not the label as sealed is refused.

A run's record holds no label. A matrix's report joins each item's label, as the set has it
when the report is made, to the verdicts of the runs.

## Matrices

A matrix names sets, judges, arms and repetitions:

```toml
[matrix]
name = "e1-evidence"
sets = ["source-fidelity/scenes-v1", "reality-checks/people-v1"]
arms = ["F"]           # C, F, and either with +<tool> after it
repetitions = 2
concurrency = 2        # runs at once; 2 when not given

[[matrix.judges]]
backend = "haiku"      # the config's [backends.judge] of that name, or default
model = "claude-haiku-5-5"
```

A cell is one item of one set under one judge and one arm, at one repetition, and is one run
of `check`, whose record holds the cell (`check`'s argument `cell`). Arm `C` measures nothing
first and offers `view` and `align`, the latter answered with its pictures alone (squint's
control); arm `F` measures the details of each pair first. `C+outline` is squint's
`D_outline`. A judge must be a backend of the config of the model it names.

Before the first run the cells still to run are forecast, at each model's settled cost of a
decision in the study (`docs/baseline/manifest.toml`), twice that under `F`, and doubled; a
forecast over what the store's allowance has left is refused before anything runs. Each run
reserves its own bound when it is admitted, and at most `concurrency` runs are under way at
once. Run again over the same store, a matrix resumes: a cell that completed is not run again;
one whose last attempt failed is reported as failed and not run again, so no later success
replaces a failure; one whose last attempt was interrupted is run as its next attempt.

`matrices/` holds squint's study matrices, ported, with the models of the Google and OpenAI
judges named, and the matrix of E1's evidence. `whizbang runs stats` and `runs export` report
on the runs a matrix made (`--matrix <name>`).
