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
  `https://raw.githubusercontent.com/markhayden/whizbang-evalsets/<images_rev>/<set id>/<path>`;
  a set that names another repository is not fetched. The repository was renamed: a set
  sealed before names it `github.com/markhayden/wafflery-evalsets`, which is read as the
  current name (`SETS_REPOSITORY_ALIASES`), and a new set names the current one. The config
  must approve that origin as a recipient (`[egress.recipients]`), or nothing is fetched; the
  request carries nothing but its address. What arrives is imported only when its SHA-256 is
  the job's pin. With `--from <dir>`, the pictures are read from a folder laid out as that
  repository is (`<dir>/<set id>/<path>`), by the same pins, and nothing is fetched.
- `whizbang eval matrix --matrix <file> --store <dir> --config <file>` runs a matrix (below).
- `whizbang eval judges --matrix <file> [--matrix <file> ...] --store <dir> --labels <dir>
  --mode judges|arms [--arm <arm>] [--rule judge-pick/1|judge-pick/2|submission/1|submission/2]
  [--set <id> ...] --out <file>` scores the runs matrices made ("Scoring judges", below).
- `whizbang eval admitted [--receipts gates]` lists what the receipts of the gates admitted
  ("Several gates on one matrix", below).

## Building a set

`tools/sets/` is the set builder (plan mE, task B1; ADR 0050), ported from squint's
`scripts/sets/` at the commit `docs/sources.toml` pins. It is a small Python program with one
package, Pillow, pinned by hash in its `uv.lock`, run on the Python the pack pins, from its
folder:

```
cd tools/sets
uv run --locked --no-build --no-config --no-python-downloads python -m whizbang_sets build \
  --plan <plan.toml> --out <owner storage>/build/evalsets/<set id> --owner <owner storage> --fake
# review <owner storage>/<set id>/contact-<family>.png and labels-draft.toml:
# set each label's status to "confirmed" or "struck", and adjudicated_on to the date
uv run --locked --no-build --no-config --no-python-downloads python -m whizbang_sets promote \
  --draft <owner storage>/<set id>/labels-draft.toml --set <the set folder> --attest "<attestation>"
```

A **plan** names the set and its families. Each family has one generator (`fal` for FLUX.2
[klein] 4B, `openai` for gpt-image-2, `gemini` for Gemini 3.1 Flash Image, or `fake`) and is
of one of three kinds:

- A **scene** family has one generated source, its protected subjects (one requirement
  each, under source fidelity), a brief, and items each made from the source by one op:
  `grade` (a mild colour grade: a pass), `restyle` (the generator makes a permitted change,
  such as the light or the season: a pass the owner confirms kept the structure), `mirror`,
  `rotate`, `crop`, and the small camera changes `tilt`, `zoom` and `keystone`, each refilled
  to the frame at the source's size (the structural rule, a fail over the whole picture), and
  `edit` (the generator removes, adds or changes something named by `removes`, `adds` or
  `alters`: the identity rule, a fail whose region is the densest patch of pixel difference;
  any other dense patch is noted for the reviewer). Any op may carry delivery damage, `jpeg`
  and `resample`, which changes no label. An item may name its own generator. Under reality
  checks the job is the candidate alone: `as_is`, `restyle`, `edit` with a `defect`
  (`anatomy`, `shadow`, `text` or `merge`, labelled RE1 to RE4) and `generate` with one.
- A **subject** family has one to four generated references and three forms of item: a
  `scene` composed from the references (a pass), an item made `from` an earlier one by an
  `edit` that changes a marking (RC2; under product fidelity PF1, PF2 or PF3 by its `aspect`),
  and a `scene` with a `lookalike` made without the references (RC3 and RC1; PF1 and PF2).
- A **brief** family has a brief, what it `asks` for and `forbids` (one requirement each) and
  the `text` it gives; each item is `generate` with a seed, from the brief or from one that
  `violates` BC1, BC2 or BC3.

The draft label comes from the construction alone: no judge or tool looks at an item to
write it. `build` writes the set folder (manifest, families, one job per item with its
pictures pinned) and, under `--owner`, the construction record, the draft labels and a contact
sheet per family; `--owner` inside a repository is refused. A set's id is
`<use case>/<name>`, and every id is lower-case letters, digits and hyphens, so none leads out
of its folder. `rebuild` makes one item again from the pictures already built, with the plan
as it now reads, and puts only its label back to draft:

```
uv run --locked --no-build --no-config --no-python-downloads python -m whizbang_sets rebuild \
  --plan <plan.toml> --out <the set folder> --owner <owner storage> --fake \
  --item <id> [--seed-offset N]
```

`promote` publishes the confirmed labels, takes the struck items out, and writes the seal that
`eval sets check` verifies; a label still a draft is refused.

**Blind sets.** A blind plan lives in owner storage and is refused inside a repository, since
an item's construction gives its label away; so is its `--draft`. Its item ids are public, so
each is its family's id and its place, `<family>-<n>`, and any other is refused.
`promote --labels-out <owner storage>` moves the labels there and leaves `labels_sha256` in
the manifest; a blind set is refused without it.

**The generators reach only approved recipients.** `--config <file>` names the whizbang config
whose `[egress.recipients]` decide where a generator may send: a request goes only to an
origin approved for every class it carries (`prompts`; with a picture to edit or compose
from, `source_images` too), a downloaded picture only from the recipient asked or one of its
upstreams, and a redirect only where the same rule allows; a key never follows a redirect, and
no proxy is used. Addresses are read as whizbang reads them (`Origin::of`: plain ASCII,
`scheme://host[:port]`), and a recipient's `account` is read and named in the ledger. The keys
are read from `FAL_KEY`, `OPENAI_API_KEY` and `GEMINI_API_KEY`.

**The spend guard.** A live build or rebuild names a ledger in owner storage, `--ledger`, one
for all of a milestone's generation, and sets its allowance once, at its first use
(`--cap-usd 8`); every later build shares it, across interruptions. `tools/sets/prices.toml`
holds, for each generator, the most that one request as the builder builds it can cost
(`usd_per_request`): at the largest side of any picture it sends or asks for (`largest_side`,
the largest source size in use) and with as many pictures sent as `pictures_sent` (up to 4
references), input tokens included, dated (`checked_on`), read from the provider's page by
the orchestrator before the first build. A generator with no row is never used live, and a
request larger than its row in either is refused. OpenAI's edits are asked for at the size of
the picture sent, so the answer is of the shape priced. Each request, each retry included, is
charged at its bound and written to the ledger, synced, before it is sent, and one that would
pass the allowance is not sent; a request with no charge is never sent. The build prints what
the ledger holds. This spend is outside every store's ledger. Where the config approves
several accounts at one origin, `--account` names the one to send as.

**Full requirements.** A protected subject of a scene family may be a table that states its
`colour`, `count` and `lettering`: `{ subject = "the red door", colour = "red", count =
"two", lettering = "OAK & ASH" }`. Each is a clause of the subject's requirement, after
" keeps ", where `per_subject` reads the subject: "The red door keeps its identity, shape,
position and extent relative to the source, and its colour (red), its count (two) and its
lettering (OAK & ASH)." `requirements = "plain"` in `[set]` writes today's wording instead, so
one plan builds both twins from the same pictures.

**Two jobs on one picture.** A scene item may give its job its own `brief`, and a `cover` op
(`prompt`, `covers`, `permitted`) lays a seasonal cover over a protected subject: a pass when
`permitted`, else a fail of that subject's identity. An item with `same_as` an earlier cover
item judges that item's picture again, under its own brief. `tools/sets/plans/
seasonal-cover-v1.plan.toml` is the pair D1 builds.

**What is printed of a blind set.** Counts and the set's id; never an item's id, a path, a
prompt, a label, a plan's text or a provider's answer. A failure is printed by its kind, a
provider's by its status, and its text is kept in `messages.log` in owner storage. A
provider's answer is never printed for any set.

**A blind plan is drawn.** The owner writes a pool in owner storage (the set, and each scene
family with its source, protected subjects, `edits` and `restyles`); `draw --pool <pool>
--seed N --out <plan>` chooses each item's construction, structural op, strength, generator
and order from the seed, writes the plan beside the pool and prints counts alone. Each family
gets a pass, an identity edit and a structural op at least; the ids are `<family>-<n>`.

**Publishing.** `WHIZBANG_OWNER_DIR=<owner storage> WHIZBANG_EVALSETS_DIR=<clone>
scripts/sets-publish.sh <set id> "<attestation>"` promotes the set built at `<owner
storage>/build/evalsets/<set id>`, commits its pictures in the clone of the pictures'
repository (whose `origin` must be the current name), pins that commit (`pin`), places the
set without its pictures under `evalsets/` and checks it. It never writes over a published
set or pictures the clone holds, and refuses a clone inside this repository or with changes
staged. Pushing the clone, the row of `evalsets/README.md` and the commit here follow. A publish
that failed part-way is run again from the start once what it did is taken back: the clone's
commit of the set (`git reset --hard HEAD~1` there, if it was made), `evalsets/<set id>`
removed, and for a blind set its `labels/` in owner storage moved aside.

`scripts/tests/test-sets.sh` runs the builder's tests and the offline dry run: a set of each
use case, a blind one and the seasonal cover pair, built with the fake generator, confirmed,
promoted, and read by `whizbang eval sets check`; then the refusals, and a publish into a
throwaway clone.

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

**Only the cells it names** (plan mE, task M1, item 1). A judge may list the arms it runs,
each by the name the matrix gives it, with its own repetitions where they are not the
judge's; a set's table may list the arms and the judges (by backend) it runs. A cell outside
any list is not a cell: it is not forecast, run, resumed or counted complete. Each set, judge
and arm keeps at least one cell, and a set's table lists arms or judges, or both. A set's
table names judges by their backend alone, so a backend it names is that of one judge of the
matrix. The lists and their order are part of the matrix's identity: the same cells listed in
another order, or a list that names every arm, make another matrix. An arm listed as a table
without `repetitions` is the arm's name alone. A matrix without lists names every cell, with
the identity it had before.

```toml
[matrix.set."source-fidelity/confirm-plain-v1"]
arms = ["C"]
judges = ["haiku", "terra"]

[[matrix.judges]]
backend = "flash"
model = "gemini-3.8-flash"
arms = [{ arm = "C", repetitions = 2 }, "C;submission=v2"]
```

Arm `C` may take the second look (`C;second_look=on`), which is one more turn after a
submission that left an item tentative, forecast at three times arm C; details stay arm F's.

An arm may freeze a tool file's method: `C+regions@eomt`, or `{ arm = "C", tools = [{ tool =
"regions", method = "eomt" }] }`, in its identity; `check` gives every call of that tool the
method, and refuses a judge's call that names another.

`eval matrix --past-forecast` prints the forecast of the cells still to run and runs anyway:
each run is admitted against the store's allowance as any run is, the first the allowance
refuses ends the matrix with the cells left named by count, and the matrix run again after a
new allowance resumes. A resumed matrix or gate never runs a failed cell again (ADR 0052): a
failure counts as an error.

**Held-out sets.** `--held-out` on `eval sets check`, `eval sets fetch`, `eval matrix`, `eval
run --gate` and `eval judges` makes every refusal name only the set's id, an item's place in
the set (from 1, never its id) and the kind of problem: no path, no parser's text, no job's
text.

`matrices/` holds squint's study matrices, ported, with the models of the Google and OpenAI
judges named, the matrix of E1's evidence, and the matrix of plan e1's format round
(`e-submission.toml`: v1 against v2 under arm C, each arm naming its format).
`whizbang runs stats` and `runs export` report on the runs a matrix made (`--matrix <name>`).

## Several gates on one matrix

A gate file of version 2 names its matrix and the matrix's identity, and `eval run --gate`
runs that matrix of exactly the gate's two arms, each run holding the gate's digest. A gate
file of version 3 (plan mE, task M1, items 3 and 4) is frozen in its matrix instead:

- **The binding runs one way.** The gate names its matrix by file alone (`[run] matrix`; a
  `matrix_identity` is refused). The matrix names each gate frozen on it, beside it, by the
  SHA-256 of the gate file's bytes, in `[gates]`:

  ```toml
  [gates]
  "e-rung3-outline.toml" = "<64 hex digits>"
  ```

  The table is in the matrix's identity, which every run holds, so a gate named there was
  frozen before the first cell. Gate files are written first, then the matrix.
- **What a gate compares.** `[baseline]` names two arms of the matrix (`control`,
  `treatment`), which may hold more arms; or two versions of a set of one use case under one
  arm (`arm`, `control_set`, `treatment_set`). A judge is compared when it runs both sides:
  both arms on a set, or the arm on both sets. Two versions must hold the same items in the
  same families by id, or nothing is scored; they are paired item by item, and an item whose
  label differs between them is counted in the receipt (`relabelled`) and scored under
  neither version's truth.
- **Hypotheses.** Each gate's `[confidence] hypotheses` is the count over every gate the
  matrix names, one for each judge a gate compares. A matrix may state the round's whole
  count at its top, `hypotheses = 21`, in its identity: at least its gates' comparisons, with
  room for the judge's rule beside them; every gate then corrects for that count and
  resamples as it needs, the larger of 20,000 and 20 × 2 × H ÷ 0.05 rounded up to a thousand
  (each tail at least 20 draws). Its `[repetitions] count` is the most of any cell it reads.
- **A catch-rate gate.** `[measure] primary = "catch_rate"` gates on the gain in the share of
  expected failures caught, resampled over the families with at least one; with
  `[repetitions] combine = "mean"` each item's catch is its mean over the repetitions. The
  guardrails are read as the file states them, on repetition 1.
- **What it admits.** A gate may state `[admits]`: `rung` (3 or 4), `kind` (`tool`,
  `use_case`, `format`, `wording` or `judge`) and `name`. Its receipt holds it, the split of
  each set it read and the day its last cell ended. `whizbang eval admitted [--receipts
  gates]` lists each thing a receipt admitted: an admitted outcome of a rung 4 gate whose sets
  are all `blind` or `heldout`, with the receipt's digest and date. A rung 3 gate, or one on a
  dev or exposed set, admits nothing.
- **Running and scoring.** `whizbang eval matrix` runs the cells, and refuses before anything
  runs when a gate file the matrix names is missing, is not the bytes its digest names, does
  not name the matrix back, or does not hold (its arms, its count, its repetitions).
  `whizbang eval run --gate <file> --labels <dir> --receipt <file>` then runs nothing: it
  scores the gate from the matrix's runs, only the cells of its two sides under the judges it
  compares, refuses while one of them has not ended, and writes a receipt of version 2, which
  adds `arm` and `relabelled` for two versions of a set. The receipt's `cells` counts every
  cell the gate read, those of relabelled items among them. Run it once for each gate.
  `eval matrix` also refuses, before any cell runs, two versions of a set whose items or
  families differ by id.

## Scoring judges

`whizbang eval judges` (plan e1, task S1) scores judges against each other, or one arm against
another within each judge, from the runs the matrices named made, and applies a decision rule
in code. It reads the store only, and never runs or sends anything.

- **What it reads.** Each cell of each matrix, by the matrix's identity in its runs (a gate's
  runs are not a matrix's), its last attempt: a completed one by its verdict, a failed one as
  an error. A cell named by two of the matrices is refused. Labels are read as `eval run
  --gate` reads them: a set that publishes its labels by its own file, with its corrections; a
  blind set's from `<labels>/<set id>/labels.toml`, held to the set's seal, the labels' folder
  at or below the sets' folder refused. `--out` under the sets' or the labels' folder is
  refused. Each run's diagnostic `rejections.json` gives its rejections by class and the cause
  it ended by: contract `rejections/1` (task T1) or `rejections/2` (task F1, which adds the
  format the run resolved, the further classes of a rejection and the recoveries). A run
  without it, or with another contract, counts as unknown, never as clean. A class, cause or
  kind the build does not know is counted as `unknown`. A diagnostic whose format is not the
  one its arm names (`submission=`) is counted as mismatched and not scored.
- **The sample.** Balanced accuracy, decided correctly (catches and passes over every item),
  catch rate, false rejects, false accepts, unsure, errors, cost per decision and median time
  use repetition 1 only. Repetition 2 gives self-agreement. The submission measures (failures,
  first rejections, rejections by first class, the further classes apart, causes, recoveries
  by kind) use every decision of the cells compared. A submission failure is a run that ended as `invalid_submission` or
  `no_submission_after_repair`. Settled spend is reported apart.
- **Modes.** `judges`: every judge under the one `--arm`, each against the best (the highest
  balanced accuracy among the eligible judges); a judge without a decision of that arm on an
  item makes the matrices incomplete. `arms`: the matrices have two arms; `--arm` names the
  control (the first arm of the first matrix when not given), and within each judge the other
  arm is compared with it. Each difference is resampled by family (seed 20261120, 2,000
  resamples, 95%). Nothing is compared when the matrices are incomplete. An interval of a
  judge's own balanced accuracy or decided correctly, or of its difference from the best, is
  not worked out when the statistic cannot be worked out on any resample (no expected fail or
  no expected pass drawn): such a resample is counted, never filled in, and under
  `judge-pick/1` it makes no pick.
- **Scopes.** All sets pooled; the `dev` sets pooled; the `blind` sets pooled (`blind-v3`
  alone today); each use case, so `source-fidelity` pools `scenes-v1`, its adversarial set and
  `blind-v3`.
- **`judge-pick/1`** (`--mode judges`), over all sets and over `source-fidelity` (the loop's
  judge). Complete first: every cell of the matrices ended and every item labelled, or no
  pick. Eligible: errors under 2% of decisions and decided correctly at least 0.80. Within the
  margin: the lower bound of the 95% interval of its balanced accuracy minus the best eligible
  judge's, the best taken again inside each resample, is at least -0.03; its median time is at
  most twice the fastest eligible judge's; its decided correctly is at least the best's minus
  0.05. The pick is the cheapest judge within the margin, ties to the lower false-reject rate.
  No pick keeps `claude-haiku-5-5` and says why; each judge's clauses are written.
- **`submission/1`** (`--mode arms`), as plan e1's task F2 freezes it: complete first, and a
  decision without the diagnostic in either arm is no decision; (a) the treatment halves the
  control's submission failures (at least 5), or else its first rejections (at least 10), or
  there are too few to tell; (b) no judge has more failures under the treatment; (c) no
  judge's balanced accuracy falls, nor its false-reject or false-accept rate rises, by more
  than 2 points. The treatment is adopted when (a), (b) and (c) hold; the record names the
  clause that settled it.
- **`judge-pick/2`** (plan mE, task M1, item 6): `judge-pick/1`'s clauses at the level the
  matrix's correction gives (the matrix states `hypotheses`, at least its gates' comparisons
  and one for each judge of the arm), at the resamples that count needs; inconclusive below
  20 families, or with a tail the resamples cannot resolve.
- **`submission/2`** (task M1, item 5): `submission/1`'s clauses over the items each judge
  scored under both arms; (a) pooled over the judges, (b) and (c) for each, (c) read on the
  family-resampled 95% interval of each change at its bound toward harm (balanced
  accuracy's lower, the rates' upper).
- **`--set <id>`**, repeatable: only those sets are scored, and completeness is over the cells
  the matrices name for them.
- **What it writes** (`judges/1`, TOML): per judge, arm and scope the measures with counts
  beside each rate and intervals for balanced accuracy and decided correctly; the comparisons;
  the rule's outcome; the matrices' names and identities; each set's split and the SHA-256 of
  its seal, labels and corrections; the commit; and each judge's backend, model and the
  versions its records report. Never an item, a family, a verdict of an item or a word of a
  judge. It prints the counts and the outcomes.
- **Plan e1's judge round** (task J1) is scored from three matrices under one arm,
  `C;submission=v1`: the format round's (`matrices/e-submission.toml`, whose cells of that arm
  and repetition 1 give the three small judges on the dev sets), `matrices/e-judges-dev.toml`
  (the other two judges on the ten dev sets) and `matrices/e-judges-blind.toml` (all five on
  `source-fidelity/blind-v3`), so every judge is scored on the same 253 items. Its record is
  `docs/results/e-judges.md`.
