# Plan: milestone E, evidence and edit quality

Draft 1, 2026-11-25, written from `main` at 0985bfd9 (the tag `m7`, the merge of #312, the
note of milestone 7). Milestone E is `SPEC.md` section 16, row E, "Evidence and edit
quality". Its first slice ran beside milestone 5 (`docs/plans/e1-judge-format-loop.md`,
`docs/plans/e1-note.md`) and chose the default judge and kept the submission format on
descriptive evidence. This plan is the rest of the milestone: every unverified tool and
recipe taken up the ladder, the evaluations that decide what milestone 8 builds, and every
carry-over row that names E (the table "Carry-over rows owned by milestone E", below).

After E comes milestone 8 (the owner's decision of 2026-11-24). This plan builds no local
fill or judge. It decides how milestone 8 scores them, and whether milestone 8 builds a
further segmenter.

**Words this plan uses.**

- **The ladder, a rung, a gate, a mark**: as `SPEC.md` section 8 has them. Rung 1 is single
  examples, rung 2 scored alone, rung 3 small end to end, rung 4 confirmation on fresh data.
  Marks are experimental, unverified and admitted.
- **An end state** is admitted, experimental or dropped: row E takes each unverified tool
  and recipe "until each is admitted, experimental or dropped". A tool that ends E
  experimental runs only when named. A dropped tool is removed from the code and keeps its
  record (section 8, "Comparing methods").
- **A cheap judge** is `claude-haiku-5-5`, the cheapest judge the build prices (0.00357
  dollars a decision on source fidelity, `docs/results/e-judges.toml`).
- **The default judge** is `gemini-4-argon`, chosen by `judge-pick/1` on dev sets and an
  exposed set (ADR 0044). It is unverified.
- **A treatment** is an arm of a matrix that adds one thing to arm C: a tool offered to the
  judge (`C+<tool>`), a detail, the second look, or a submission format.
- **The fresh set** is `source-fidelity/confirm-v1`, built in this milestone (B1, D1): new
  families, disjoint from every set used on rungs 2 and 3, seen by no agent, its whole folder
  kept in owner storage until its one confirmation round (R4) has run. **Its plain twin**,
  `source-fidelity/confirm-plain-v1`, is the same items and pictures with requirements written
  as today's sets write them, without colour, count and lettering.
- **Owner storage** is the owner's private folder outside every repo. Its path is never
  written down; commands name it `<owner storage>`.
- **Settled** is what a store's ledger settled for a round, read from `whizbang spend status`
  before and after it. It is what counts against the milestone's 100 dollars.

## What the owner decided

1. **The order after milestone 6 is 7, then E, then 8** (2026-11-24, `SPEC.md` section 16).
   E starts when 7 ends, and 8 starts when E ends.
2. **The blinded picking rounds of milestone 3 moved to E** (2026-11-19, the carry-over row
   on them). The five recipes that end in a fill have their visual quality "not yet
   confirmed by a person" until then.
3. **The owner's own pictures moved to E** (2026-11-20, plan m4, "What the owner decided",
   6), where each tool is tested hard on them.
4. **E's first slice ran beside milestone 5** (2026-11-20, `docs/plans/m4-note.md`), and
   settled 36.12 dollars of the milestone's 100 (`docs/plans/e1-note.md`, "Spend").
5. **Spend controls stay a guard and a record** (`SPEC.md` section 6). Nothing new is built
   to hold spend beyond what a round needs to stay inside its cap.
6. **A plan is approved by two or more outside review rounds** (`AGENTS.md`, "Working with
   the owner"). Building starts once they are recorded, and the owner is told.

## What the spec says, and where it is silent

**What it says.** Each of these binds the tasks below.

- Section 8, the table of kinds: a model-based recipe or a tool offered to a judge climbs all
  four rungs; a pixel operation or a control (`view`, `align`) climbs rungs 1 and 2 and "is
  complete when its record shows rungs 1 and 2". "Every squint comparison is measured
  against these two."
- Section 8, "Marks": experimental, "runs only when named"; unverified "may be a default
  while nothing admitted replaces it"; admitted "cleared its gate". "A tool that fails its
  gate is fixed and tried again, kept as experimental, or dropped." "Earlier evidence
  counts." Each mark is "shown by `whizbang describe`".
- Section 8, "Comparing methods": one name, a `method` setting, the same data and script; a
  method that loses is removed from the code and kept in the record.
- Section 8, "Gates": frozen before a rung 3 or 4 run; the defaults (balanced accuracy, a
  gain of at least 3 points with the corrected lower bound above zero, false rejects and
  false accepts under 3 points more, errors under 2%, cost at most double, at least 20
  families); too little data is "inconclusive", never a pass. An edit tool's gate is task
  success, preservation outside the mask and blinded quality.
- Section 8, "Scoring edits": squint may score a finished edit on rung 3; a rung 4 result
  for an edit tool rests on labels or blinded human review that squint did not produce.
- Section 8, "Evaluation data": `blind-v3` is exposed and never admits a tool; rung 4 needs
  fresh data, "new families, disjoint from every set used on rungs 2 and 3, unseen by any
  agent", each use recorded with its hypothesis; sealed sets are never edited, and changed
  jobs or labels make a new set version; nothing public discloses an item-level answer.
- Section 9: a judge's decision is never read from the cache in evaluation; a failed attempt
  "is reported, never replaced by a later success".
- Section 4, "Segmenters" and "Experimental tools": a model is one adapter and one catalogue
  entry, added with no edit to an existing tool's logic; a tool file is offered to a judge
  only when a run names it.
- Section 6: a limit is enforced, an estimate is advice; an allowance is reset only by the
  owner's command. Section 15: 100 dollars a milestone on approved recipients; more, a new
  recipient, or a reset is the owner's; using a held-out set is asked first; never view the
  items of a held-out set; never commit owner-only material.
- Section 16, row E, quoted whole: "Runs after milestone 7 and before milestone 8 ... Its
  evaluations decide what 8 builds: a further segmenter, and how local fill and judge are
  scored. Every unverified tool and recipe is taken up the ladder, squint's first and the
  most doubtful first, until each is admitted, experimental or dropped. Per-subject
  comparison climbs rungs 3 and 4 on a fresh confirmation set; `regions`, `classes` and
  `outline` are each brought in as experimental tools and scored on rungs 1 and 2, on SAM 3
  where that works and on their old segmenter as a catalogue entry where it does not, and
  those that pass go on; the submission format is redesigned so small models can follow it;
  job requirements carry colour, count and lettering in a new set version. Each change is
  admitted through its gate."
- Section 17: the checklist submission format was "ported as is, then redesigned through the
  ladder".

**Where the spec and the code disagree, or where the spec is silent.** Each is settled by a
task item or a question at the end; none is settled silently.

1. **Marks that `whizbang describe` does not show.** Section 8 says each tool the upper rungs
   apply to "carries one mark, shown by `whizbang describe`". The code reads marks only from
   ten squint records (`crates/whizbang-squint/src/marks.rs`, `RECORDS`), so the description
   lists nine marks (`docs/reference/description.json`, `marks`). The records of `remove`,
   `remove_prompt`, `repair`, `repair_area`, `seam`, `remove_checked`, `segment`, `fill`,
   `per_subject` and `tiles` state a mark that `describe` never shows, and the rascal records
   open with `**Mark.** Experimental`, which `marks::stated` reads as no mark. Fix: A0,
   item 3.
2. **`align` is a control, and is marked unverified.** Section 8 names `align` with `view` as
   the control, to which rungs 3 and 4 cannot apply, complete when rungs 1 and 2 are shown.
   `view`'s record says "Mark. None"; `align`'s says "Unverified ... As a control, rungs 3 and
   4 do not apply", and `describe` lists it unverified. Fix: A0, item 3: `align` states no
   mark and is complete, as `view` is.
3. **Which gate admits.** Section 8 says a gate is frozen before "a rung 3 or 4 run" and that
   admitted means "cleared its gate", while `blind-v3`, the only set with 20 families a rung
   3 run could use, "never admits a tool". The plans have read this as: dev and exposed sets
   admit nothing (`gates/details-first.toml`, its head). Fix: A0, item 4 writes it into
   section 8: a rung 3 gate decides whether a tool goes on; only a rung 4 gate on fresh data
   admits.
4. **What rungs 3 and 4 are for a use case of `check`.** The ladder's squint column describes
   an evidence tool that "improves" a judge. A use case is the judge's own procedure, with
   nothing to improve on, yet each of the five records says it "climbs rungs 3 and 4 with
   milestone E". Fix: A0, item 4 writes into section 8 that a use case's rung 3 is a judge
   round on dev and exposed sets under a frozen rule (done for all five by plan e1's J1), and
   its rung 4 is `judge-pick/1` on a fresh set of that use case: the use case is admitted
   with the judge the rule picks there.
5. **Unverified as an end of E, for four use cases.** Row E allows only admitted,
   experimental or dropped. A use case cannot be dropped or made experimental without taking
   `check` away from its users, and section 8 lets it stay a default while unverified. Rung
   4 needs a fresh set of 20 families for each use case. This plan builds one, for source
   fidelity, where every evidence tool of the ladder works (`per_subject`, `regions`,
   `classes`, `outline`, `change_map`, `edge_similarity` all compare an aligned pair). It
   proposes that the other four use cases (`reference-consistency`, `reality-checks`,
   `brief-compliance`, `product-fidelity`) end E unverified, with a carry-over row for their
   fresh sets, and asks the owner ("I need", item 3). With them, `tiles`, which serves only
   jobs of one image, ends E experimental.
6. **The pictures' repository against sealed sets.** The carry-over row on the repository's
   old name asks that "each [set] names the repository's current name, the pins unchanged".
   Each set's `images_repo` is in its `set.toml`, which its seal covers, and sealed sets are
   never edited (section 8). Fix: B1, item 6: the new sets name the current name, and the
   reader takes the old name as an alias of it for sealed sets, with a test, instead of
   editing them.
7. **The retry of a failed cell.** The carry-over row on resumed matrices asks for "a rule
   for retrying a judge's failed cell". Section 9 already rules that a failed attempt "is
   reported, never replaced by a later success". Fix: M1, item 4: no retry, by that rule, in
   an ADR line; the row closes with it.
8. **A gate compares two arms of one matrix.** The code refuses a gate whose matrix has other
   arms (`crates/whizbang/src/gating.rs`: "the matrix's arms are not exactly the gate's
   control and treatment"), so every treatment pays again for its own control, and a gate
   cannot compare two versions of a set. Not a disagreement with the spec, but E's spend
   depends on it. Fix: M1, items 1 and 2.
9. **A set builder.** Section 8 needs fresh sets and new set versions; whizbang has none of
   squint's set toolkit (squint `scripts/sets/`, `squint-sets build`, its `docs/evalsets.md`,
   "Building a set", at the commit `docs/sources.toml` pins), and the carry-over list neither
   ports nor drops it. Fix: B1 ports it.

## Choices this plan makes

1. **Squint first, then rascal; in each, the most doubtful first.** The ladder's rounds run in
   that order: R2, R3, F2 and R4 (squint), then V1, R5 and R6 (rascal). Code tasks that no
   round waits on (P0, L1, L2, C1, P1) may be built beside the squint tasks; their rounds wait.
   "The most doubtful" is ranked by the evidence each tool already has, in the table "The
   order of the ladder", below. The most doubtful are built and screened first, so that a
   tool that cannot pass stops before money is spent on it.
2. **One fresh set, for source fidelity, about 110 items in 22 families.** Twenty families is
   the gates' minimum; two more leave room for a family struck in review. It is built with
   full requirements (colour, count and lettering), and its plain twin holds the same items
   with today's requirements, so that the requirement change has a gate of its own on fresh
   data at the price of two cheap judges.
3. **One confirmation round, one matrix, several gates.** R4 runs arm C once for every judge
   and each treatment beside it, and each gate reads its own two arms (M1, item 1). Every
   hypothesis of the round is counted in one Bonferroni correction, frozen before the run.
   The fresh set is used once, for the hypotheses its gate files state, and is exposed after.
4. **Rung 3 runs on `blind-v3` under a cheap judge.** It is the only set with 20 families
   that rung 4 does not need fresh: 143 items, 24 families, labels in owner storage. It is
   exposed, so a rung 3 gate decides only whether a tool goes on. Rung 2 stays on the
   source-fidelity dev sets, as `per_subject`'s did.
5. **Rung 4 judges are cheap and from two providers.** A tool's gate on fresh data is run
   under `claude-haiku-5-5` and `gpt-6.1-terra` ("improves judges across models"), and must be
   admitted for both. The format's gate adds `gemini-3.8-flash` and the default judge. The
   judge's gate runs all five judges under arm C.
6. **The submission format is redesigned once more, as v3, from what failed.** Plan e1's
   format round counted every rejection by class (`docs/results/e-submission.md`): under v2
   Haiku's first rejections rose to 93 in 220, 77 of them `finding_cites`, and Flash ended 14
   runs as faults. v3 keeps v1's list form, which no provider faulted on, and v2's one repair
   and recoveries, and reads a finding whose citations name no held call as the finding with
   those citations dropped, recorded as a recovery, instead of a rejection (F1). Its dev
   round (F2) decides which of v2 and v3 goes to the gate on fresh data.
7. **The old segmenter comes back as a catalogue entry for what SAM 3 cannot do.** `outline`
   and `regions` are written on SAM 3's phrase requests first (T1). `classes`, which asks
   what classes cover a box, needs a segmenter that labels every pixel from a closed list,
   which SAM 3 does not; so does `regions`' class disagreement. The fixed-class segmenter
   squint used, EoMT large trained on ADE20K (squint `tools/python/MODEL.md`), comes back as a
   catalogue entry with a new feature, labels for every pixel (T2), and `regions` and
   `outline` gain a `method` that uses it. Rung 2 chooses each tool's method (R2).
8. **Edits climb on what exists, then on the owner's photos.** Rung 2 is finished by the
   owner's picks of the pages milestone 3 already drew (V1). Rung 3 runs each recipe on a
   sample and scores it with `check` under the default judge (R5). Rung 4 runs on the
   owner's own photos and rests on the owner's blinded marks, which squint does not produce
   (R6).
9. **What milestone 8 builds is decided by rules frozen here** (W1): whether a further
   segmenter is built, from a comparison of the catalogue's segmenters on the truth masks of
   `segmentation/segment-01`; and the frozen protocols milestone 8 scores local fill and a
   local judge by.
10. **Owner-only material is read only by the orchestrator, only through the commands that
    take it** (`--labels <owner storage>`, `--root <owner storage>`, `eval sets fetch --from`,
    `eval loop score --labels`, `lab decode`), and never opened, printed or copied. Only
    aggregates are committed, each record held to its numbers and to "aggregates only" by an
    evidence test.

## The order of the ladder

Ranked from the evidence each already has. Squint's tools first, then rascal's recipes.

| Order | Tool or recipe | Mark today | Why it is doubted | Path in E |
|---|---|---|---|---|
| 1 | Details first (`--details first`) | Experimental, off by default | Its gate rejected it in every run, on every judge (`docs/results/m3-gates.md`) | No fix is proposed. Ends E experimental, recorded (A0, item 5); `resize`, offered only under it, ends with it |
| 2 | `outline` | Not ported | The 2026-11 study: it ranked noise above the real change; no gain on Argon; Haiku's decided-correctly fell from 68.5% to 42.7% (`docs/results/2026-11-tool-study.md`) | T1, R2, R3, R4 |
| 3 | The second look | Option, off by default | Admitted nothing on dev sets; on reality-checks it passed pictures that fail (`docs/results/m3-unsure.md`) | R3 and R4 on source fidelity; ends experimental for the other use cases ("I need", item 3) |
| 4 | `tiles` | Experimental | Admitted nothing on dev sets; serves only jobs of one image | Ends experimental: no fresh set of one image is built (proposal 5 above) |
| 5 | `classes` | Not ported | No evidence at all; it only reads what `regions` points at | T2, R2, R3, R4 |
| 6 | `edge_similarity` | Unverified | Rung 2: AUC 0.73 (0.57 to 0.97) on 15 items, 5 errors | R3, R4 |
| 7 | `change_map` | Unverified | Rung 2: AUC 0.77 (0.61 to 0.94) on 15 items, 5 errors | R3, R4 |
| 8 | `regions` | Not ported | One gain, on Flash, that does not survive a correction across the study; lower on Haiku and Opus | T1, T2, R2, R3, R4; the repeat of the study's comparison (tier 2, "Spend") |
| 9 | `per_subject` | Experimental | Rung 2: AUC 1.0 on 16 items, a small sample | R3, R4 |
| 10 | The submission format | v1 default; v2 a setting | v2 cut failures but moved each small judge's accuracy by more than 2 points | F1, F2, R4 |
| 11 | The default judge, and source fidelity | Unverified | Argon's lead comes from `blind-v3`, an exposed set; on dev sets alone it trails Flash | R4 |
| 12 | The other four use cases | Unverified | No fresh set | End E unverified, if the owner agrees ("I need", item 3) |
| 13 | `remove_checked` | Experimental | Which attempt it keeps is not scored; 33 of Argon's 40 runs on segment-01 ended `fail` (`docs/results/e-loop.md`) | L1, L2, V1 (the contact sheet), R5, R6 |
| 14 | `seam` | Experimental | Its decode is owed; a judge cannot look into a panorama (`view` refuses any box on one); its protocol cannot reach `not_worse` | P0, P1, V1, R5 |
| 15 | `repair_area`, `repair`, `remove_prompt`, `remove` | Experimental | Visual quality owed (`docs/results/m3-edits.md`) | V1, R5, R6; a clicked removal beside `remove` (C1) |
| 16 | `segment` | Experimental | Rung 2 not shown | W1, item 1: mask agreement with segment-01's truth masks |
| 17 | `fill` | Experimental | Rung 2 not shown | Credited from the recipes' rung 2 once V1 decodes them (every fill there is Bria's through fal) |

`view` and `align` are the control: complete with rungs 1 and 2 (A0, item 3; `view`'s
panorama side, P1).

## Acceptance: the row's clauses, each with its task and its evidence

| Clause | Task | Shown by |
|---|---|---|
| Its evaluations decide what 8 builds: a further segmenter | T2, W1 | W1's record and ADR: the comparison on segment-01, the rule frozen here, its outcome |
| ... and how local fill and judge are scored | W1 | The frozen protocol and gate files for milestone 8, committed, each named by its digest |
| Every unverified tool and recipe taken up the ladder, squint's first, the most doubtful first, until each is admitted, experimental or dropped | A0, R2, R3, R4, V1, R5, R6 | Each record's "Mark and history" names its end state and the receipt that set it; `describe` shows every mark (A0, item 3); `whizbang eval admitted` lists what the receipts admitted (M1, item 3); N1's table of every tool and its end |
| Per-subject comparison climbs rungs 3 and 4 on a fresh confirmation set | R3, D1, R4 | The rung 3 receipt on `blind-v3`; the rung 4 receipt on `confirm-v1` |
| `regions`, `classes`, `outline` brought in as experimental tools, scored on rungs 1 and 2, on SAM 3 where that works and on their old segmenter as a catalogue entry where not; those that pass go on | T1, T2, R2, R3, R4 | Each record's rung 1 examples, run as tests; R2's rung 2 table for each method; R3 and R4's receipts for those that pass |
| The submission format redesigned so small models can follow it | F1, F2, R4 | F2's record; the format's gate receipt on `confirm-v1` |
| Job requirements carry colour, count and lettering in a new set version | B1, D1, F2, R4 | `source-fidelity/scenes-v2` and `adversarial-v2` sealed beside v1; the requirement gate's receipt (`confirm-plain-v1` against `confirm-v1`) |
| Each change is admitted through its gate | M1, R4 | Every receipt committed under `gates/`; `whizbang eval admitted` |

## How the work is split

Twenty-two tasks. Sizes are changed lines of source, tests and hand-written documents, by
milestone 3's rate (about 2,500 for a large one). The orchestrator's rounds are named as
theirs; builders make no live call.

| Task | What | Builds on | Size |
|---|---|---|---|
| A0 | The entry check: the Google prices; E's ledger; every mark shown by `describe`, `align` as a control; the spec's wording for gaps 3 to 5; details first's end | `main` | Small: about 600 |
| M1 | The machinery: several gates on one matrix; a gate across two versions of a set; the list of what was admitted; no retry of a failed cell; the judge round's figures held to their TOML | `main` | Medium: about 1,500 |
| B1 | The set builder, ported, with full requirements and a spend guard; the seasonal cover pair; the pictures' repository's name | `main` | Large, two pull requests: about 1,400 (the port), then about 1,100 (full requirements, the guard, the pair, the name) |
| T1 | `outline`, then `regions`, as tool files on SAM 3; the rules' `[hints]` and the checklist's evidence from regions | `main` | Large, two pull requests: about 1,100 (`outline`), then about 1,200 (`regions` and the rules) |
| T2 | The fixed-class segmenter as a catalogue entry that labels every pixel; `classes`; the `method` of `regions` and `outline` | T1 | Large, two pull requests: about 1,400 (the adapter, its entry, conformance), then about 1,000 (`classes`, the methods) |
| F1 | Submission v3; Haiku's invalid submissions counted by rule | `main` | Medium: about 1,400 |
| D1 | The new set versions and the fresh set (the orchestrator's, with the owner) | B1 | Medium: about 500, and the sets' files |
| R2 | Rung 2 of the new tools and their methods (the orchestrator's) | T2 | Small: about 400, records |
| R3 | Rung 3 of squint's tools on `blind-v3` under a cheap judge (the orchestrator's) | M1, R2 | Medium: about 600, gate files and records |
| F2 | The dev round of the format and the requirements (the orchestrator's) | F1, D1 | Small: about 400, a matrix and records |
| R4 | Rung 4: the confirmation round on the fresh set (the orchestrator's) | M1, D1, R3, F2 | Medium: about 900, gate files, receipts, records, ADR 0044 amended |
| P0 | The pairs page: sizes that follow from dimensions alone; the engine its test needs; `not_worse` | `main` | Small: about 500 |
| V1 | Visual quality: the picking rounds of milestone 3 and the loop's contact sheet (the orchestrator's, with the owner) | P0 | Small: about 400, records |
| L1 | The loop's record and evidence: a closed set of reasons; located retries that added nothing; the tenth | `main` | Medium: about 1,000 |
| L2 | The located retry, whole: the phrase delimited; a place of any size; a polygon as its pixels; one mask at a time; one decode | L1 | Medium to large: about 1,600 |
| C1 | The clicked removal: measured on segment-01, then built if the rule asks | `main` | Medium: about 900 if built, 200 if not |
| P1 | Panoramas: `view`'s panorama side and its mapping; locations through every binding; the scaled slack; a reduced crop with transparency; the seam's two record lines | `main` | Large, two pull requests: about 1,500 (`view`'s panorama side, the mapping, its ADR), then about 1,000 (locations, the slack, alpha, the records) |
| R5 | Rung 3 of the edit recipes (the orchestrator's) | V1, L2, C1, P1 | Medium: about 600, gate files and records |
| R6 | Rung 4 of the edit recipes on the owner's photos (the orchestrator's, with the owner) | R5 | Small: about 400, records |
| W1 | What milestone 8 builds: the segmenters compared; the protocols for local fill and judge | T2, R4, R6 | Medium: about 700, an ADR, records, protocol files |
| N1 | The note | all | Small: about 500, no code |

**Waves.** Each from `main` once what it builds on has landed.

1. **A0, M1, B1, T1 and F1**, side by side. They share no source file but
   `docs/carry-over.md`. Beside them, the code tasks no squint round waits on: **P0, L1, C1**
   (its measurement waits for nothing paid) and **P1**.
2. **T2** when T1 has landed; **D1** when B1 has landed (the owner reviews the contact
   sheets); **L2** when L1 has landed.
3. **R2** when T2 has landed; then **R3** (and M1); **F2** when F1 and D1's dev versions
   have landed.
4. **R4** when D1's fresh set, R3 and F2 are in, and the owner has said yes to its use.
5. **V1** (it may start any time after P0, as the owner's time allows); **R5** when V1, L2,
   C1 and P1 are in; then **R6**.
6. **W1**, then **N1**.

**Shared files, each with one owner in a wave.** `docs/carry-over.md`: each task changes only
the rows the table "Carry-over rows owned by milestone E" gives it; the second to land in a
wave merges `main`; N1 writes the rest. `crates/whizbang-squint/src/marks.rs`: A0, then T1
and T2 add their records. `crates/whizbang-eval/src/gate.rs` and `crates/whizbang/src/
gating.rs`: M1 alone. `crates/whizbang-squint/src/submission.rs` and `check.rs`: F1 alone.
`pack/`: T1 (tool files), then T2 (the adapter). `crates/whizbang-rascal/src/checked.rs`: L1,
then L2. `crates/whizbang-squint/src/view.rs` and `crates/whizbang-core/src/identity/`: P1
alone. `docs/reference/`: whoever changes the description runs `just docs`. This plan's own
file: a task changes only its own section and box.

**Review rule.** Every pull request is reviewed whole by a reviewer that did not write it,
who runs `just check` and `just scan-range` over `base..head`, and merged by the orchestrator
on green CI. Each task names what its reviewer reads hardest. A record of a round is reviewed
for aggregates only: no item, family, picture, label, phrase of the owner's or path.

**Live runs are the orchestrator's.** Builders make no provider call, load no real model in
CI, and never read owner storage. Every round is run by the orchestrator on the owner's Mac by
the committed command its task names, under `scripts/heavy.sh`, after the prices step (A0,
item 1). Local models (SAM 3, Tiny, the fixed-class segmenter) run on the Mac's GPU and spend
nothing.

**Test time.** At most 15 seconds of fast-tier test time for the milestone, each task's share
reported in its pull request. Every test that calls a provider or a real model is
`#[ignore]`d or in the model tier.

## A0. The entry check (small)

1. **The Google prices** (the carry-over row on the Google rows of prices). Before the first
   round that asks a Google judge (R3 asks none; F2 asks Flash), and on or after each row's
   date, the orchestrator reads the provider's price page: `gemini-4-argon`'s row on or after
   2026-11-30, `gemini-3.8-flash`'s on or after 2026-12-31. A row past its date is changed
   with its date and ADR 0026 in its own pull request, and the round runs on a binary with the
   change. A round that would cross a date waits for the read. This task opens the procedure;
   each round's task does the read it needs.
2. **E's ledger.** The milestone's settled spend so far, from the first slice's note: 36.12
   dollars. What is left of the milestone's 100: 63.88. What each store has left, read by
   `whizbang spend status`: the evaluation store about 40.94 (70 less 29.06), the loop store
   about 17.71 (25 less 7.07 and milestone 5's 0.22). Written into this plan's "Spend" as
   read, before any round.
3. **Every mark shown, and `align` as a control** (gaps 1 and 2). `marks::stated` reads both
   phrasings a record uses, `**Unverified**` and `**Mark.** Experimental`; `RECORDS` gains
   every record whose tool the upper rungs apply to, rascal's included (`remove`,
   `remove_prompt`, `repair`, `repair_area`, `seam`, `remove_checked`, `segment`, `fill`) and
   the tool files (`per_subject`, `tiles`); a test reads every record under
   `docs/reference/tools/` and fails when one that states a mark is not in `RECORDS`.
   `align.md`'s "Mark and history" opens `**Mark.** None`, as `view.md`'s does, naming it
   complete as a control; `evidence::` tests that held `align` unverified are changed to
   hold it unmarked. `just docs` writes the description again.
   `marks::every_record_that_states_a_mark_is_shown_by_describe`;
   `marks::the_control_carries_no_mark`.
4. **The spec's wording for gaps 3, 4 and 5.** `SPEC.md` section 8 gains two sentences under
   "The ladder" (a rung 3 gate decides whether a tool goes on; only a rung 4 gate on fresh data
   admits) and one under the table of kinds (a use case's rung 3 is a judge round under a rule
   frozen before it; its rung 4 is `judge-pick/1` on a fresh set of that use case). If the
   owner says yes to "I need", item 3, row E gains: "the use cases without a fresh set end E
   unverified". This is the spec brought in line with a reviewed plan.
5. **Details first's end.** Its record and `docs/reference/tools/source-fidelity.md` say it
   ends E experimental: no fix was proposed, and its gate rejected it in every run. `resize`,
   which `check` offers only under it, ends E experimental with it, its record saying so.
   The "Evaluations" row on `tiles`, the second look and details first is narrowed with that
   line.

- [ ] Done when: the two tests pass; `describe` shows every mark; the spec's sentences are in;
  the ledger's figures are in this plan. **Reviewer reads hardest:** that no record's mark
  changed but `align`'s.

## M1. The machinery (medium)

1. **Several gates on one matrix.** Gate file version 3: `[baseline]` names its control and
   treatment among the matrix's arms, and the matrix may hold more; a gate reads only its two
   arms' cells. `[confidence] hypotheses` counts every hypothesis of every gate frozen on the
   same matrix, and `eval run --gate` refuses a set of gates on one matrix whose counts differ
   or are fewer than their hypotheses together. Version 2 files read as before. One run of the
   matrix, then each gate scored from the store: `whizbang eval run --gate <file>` scores the
   cells it finds, and runs only those still to run.
   `gate::a_gate_reads_its_two_arms_of_a_wider_matrix`;
   `gate::gates_on_one_matrix_count_their_hypotheses_together`.
2. **A gate across two versions of a set.** `[baseline]` may name `control_set` and
   `treatment_set` instead of two arms: two sets of one use case whose items and families are
   the same by id, under one arm. Refused when the ids or families differ.
   `gate::a_gate_compares_two_versions_of_a_set_item_by_item`.
3. **What was admitted** (the carry-over row on admissions). `whizbang eval admitted
   --receipts gates/` reads every receipt and prints each tool, use case, format or judge a
   rung 4 gate admitted, with the receipt's digest and date; a receipt of a rung 3 gate, or
   on a set whose split is not `blind` or `heldout`, admits nothing. Each record's "Mark and
   history" names the receipt by digest, and a test holds the records' marks to the list.
   `admitted::a_rung_3_receipt_admits_nothing`; `admitted::the_records_marks_are_the_receipts`.
4. **No retry of a failed cell** (the carry-over row on resumed matrices). ADR 0049's
   neighbour, a new short ADR "Gates of milestone E", records items 1 to 3 and: a resumed
   matrix or gate never runs a failed cell again, by section 9 ("a failed attempt is
   reported, never replaced by a later success"), and a failure counts as an error, which the
   2% guardrail sees. The row closes with that line.
5. **The judge round's figures held to their TOML** (the carry-over row on figures copied by
   hand). A test reads `docs/results/e-judges.toml` and requires each use case record's
   "Another judge" table and "Speed and cost" line, ADR 0044's figures and the two guides'
   figures to be the ones it gives. `evidence::the_judges_figures_outside_their_record_are_its_own`.
6. **A set read from owner storage before it is published.** The matrix, gate and judges
   commands already take `--root` for the sets and `--labels` for blind labels; a test holds
   that a set under another root runs and scores, that a label folder at or below the sets'
   folder is refused, and that no record or receipt names the root's path.
   `gating::a_set_under_another_root_is_scored_and_its_path_is_nowhere`.

- [ ] Done when: the tests pass and the ADR is in. **Reviewer reads hardest:** that a version 2
  gate scores exactly as before; that a gate cannot read a third arm's cells.

## B1. The set builder, ported (large, two pull requests)

Ported from squint's `scripts/sets/` (`squint-sets build`, `rebuild`, `promote`, and
`scripts/sets-publish.sh`) at the commit `docs/sources.toml` pins, to `tools/sets/` here,
with its own `uv` project and lock, run with `uv run whizbang-sets`. Every porting commit
names the source path and commit.

**First pull request: the port.**

1. **Plans, families and construction ops** as squint has them: scene, subject and brief
   families; `grade`, `restyle`, `mirror`, `rotate`, `crop`, `tilt`, `zoom`, `keystone`,
   `edit`, delivery damage; the fake generator; the draft label from the construction alone,
   no judge and no tool run on an item.
2. **Generators** `fal`, `openai` and `gemini`, the three approved for this use in squint, at
   the recipients `SPEC.md` section 15 approves (fal, OpenAI's API, Google's API). Keys come
   from the environment by name; the toolkit never writes one.
3. **The public and the private halves.** `build` writes the set folder (manifest, families,
   jobs with pinned pictures) and, to `--owner <owner storage>`, the construction record,
   draft labels and a contact sheet per family. For a blind set the plan itself lives in owner
   storage and is drawn by a seeded script that the building agent never reads; `promote`
   with `--labels-out` moves the labels to owner storage and leaves `labels_sha256`.
4. **Checked as whizbang checks.** The set it writes passes `whizbang eval sets check --root`;
   the offline dry run with the fake generator runs in `just test-scripts`.
   `scripts/tests/test-sets.sh: the dry run builds a dev set that whizbang's checker reads`.

**Second pull request: full requirements, the guard, the pair, the name.**

5. **Full requirements** (the carry-over row on colour, count and lettering). A protected
   subject in a plan may state its `colour`, its `count` and its `lettering`; each stated one
   is written into that subject's requirement as a checkable clause ("the two red doors, with
   the lettering OAK & ASH, keep their colour, count and lettering"). A plan flag
   `requirements = "plain"` writes today's wording, so one plan builds both twins from the
   same pictures. Tests hold both wordings.
6. **The pictures' repository's name** (gap 6; the carry-over row on its old name). New sets
   name the current repository; `SETS_REPOSITORY` names it; the reader takes the old name as
   an alias for sets sealed before, with a test that every set resolves to the one
   repository. No sealed file changes.
7. **A spend guard.** A table of a bound price per generated picture for each generator,
   dated and read from each provider's page by the orchestrator before the first build; a
   build refuses to start when its plan's pictures times their bounds pass `--cap-usd`, and
   stops when what it has asked for reaches it. The construction record states each call's
   bound; the build prints the total. This spend is outside every store's ledger and is
   counted in the milestone's spend by the orchestrator from that total.
8. **The seasonal cover pair** (the carry-over row on a seasonal cover). A dev plan of one
   family whose `restyle` lays snow over a protected subject, with two jobs on the same
   pictures: one whose brief permits a seasonal cover (a pass), one that does not (a fail).
   Built by D1 from fresh images, as a fixture of `check`'s use-case tests.

- [ ] Done when: both pull requests are merged, the dry run passes in `just check`, and the
  alias test passes. **Reviewer reads hardest:** that no plan, label or contact sheet of a
  blind set can be written inside the repo (`--owner` inside the work tree is refused).

## T1. `outline`, then `regions`, as tool files on SAM 3 (large, two pull requests)

Ported from squint's `tools/python/src/squint_tools/outline.py` and `regions.py` at the
pinned commit, as tool files in ADR 0032's shape under `pack/tools/`, beside `per_subject.py`.
`outline` first: it is the most doubted of the three.

1. **`outline` on SAM 3.** For a protected subject (the job's `must_hold` requirement, or the
   class a hint names), the subject's mask on each frame of an aligned pair, from SAM 3's
   phrase request through the tool context; then squint's comparison of the two shapes:
   overlap, areas, how far the boundary moved and where, and the pieces added or removed as
   regions on `frame`. Its constants as squint's, each named in the record.
2. **`regions` on SAM 3.** The photometric change map, as `change_map` computes it, joined
   with SAM 3's masks of the job's subjects where squint used class disagreement; connected
   components as boxed regions, scored and capped at `max_regions`. Squint's class labels on
   a region ("floor -> swimming pool") come only with T2's method.
3. **Records** for both, at `docs/reference/tools/<name>.md`, each with two worked examples run
   as tests (rung 1, one a case it handles badly if one is known), marked experimental, and
   listed in `marks.rs`.
4. **The rules' parts regions need** (the carry-over row on what the port of the use cases
   left). `[hints]` (`regions`' `preserve` and `ignore` classes) and the checklist's evidence
   items from a tool's regions (`checklist.rs`, `ingest`) are ported with tests. A profile and
   `extends`, a consumer's own rule files and `[screen]` are dropped with a line in the row:
   no arm, set or user has asked for them since the port, and a rule file that holds one is
   still refused when read.

- [ ] Done when: both tools run under `whizbang try` and `eval tool` against the fake models;
  their records' examples run as tests; the row is closed. **Reviewer reads hardest:** that a
  tool file reaches pictures and SAM 3 only through its context, and that its output is held
  to its declared bounds.

## T2. The fixed-class segmenter, `classes` and the methods (large, two pull requests)

By the `add-adapter` skill. The model is the one squint used (squint `tools/python/MODEL.md`):
EoMT large, trained on ADE20K at 512, weights MIT and code MIT, backbone Apache-2.0, through
`transformers`, at a pinned revision whose weights are checked by hash.

**First pull request: the adapter and its entry.**

1. **A catalogue entry**, `eomt-ade20k`, with its files, hashes, licences and size, from
   `https://huggingface.co`, the origin SAM 3's entry already uses (ADR 0017). If any of its
   files is served from an origin the config does not already approve, the builder stops and
   the orchestrator asks the owner ("What the owner provides").
2. **A new feature, `labels`** (the "Evaluations" row "Segmentation that labels every pixel |
   When a tool needs it": `classes` needs it). A request for labels returns, for the whole
   picture, a class per pixel from the model's closed list and each class's confidence, as
   one artifact; the feature is declared, and a model that does not declare it refuses the
   request (ADR 0006). The entry also answers phrase requests whose phrase is one of its
   class names, and refuses any other phrase, naming the list.
3. **Conformance** with fixtures and the fake adapter, and the model tier's test on the Mac
   (`just test-models`): one labels request on a fixture, its classes held to a tolerance.
   An ADR, "The fixed-class segmenter", records the model, the feature and why it is a
   catalogue entry and not the default.

**Second pull request: `classes` and the methods.**

4. **`classes`**, ported from squint's `classes.py` as a tool file: for a box on `frame`, the
   classes covering it on each side with their share and mean confidence, and a region of
   kind `class_difference` when the dominant class differs. Its record, two examples as tests,
   marked experimental.
5. **`method`** on `regions` and `outline` (section 8, "Comparing methods"): `sam3` (T1's,
   the default) or `eomt` (squint's class disagreement for `regions`; squint's class score
   for `outline`). Each method's examples run in the record.

- [ ] Done when: conformance passes with fakes; the model tier passes on the Mac; `classes`
  and both methods run under `eval tool` against the fakes. **Reviewer reads hardest:** that
  adding the entry edited no existing tool's logic (section 18, criterion 4).

## F1. Submission v3 (medium)

1. **Why small judges fail, by rule** (the carry-over row on Haiku's invalid submissions).
   From the format round's diagnostics, by `whizbang eval judges` and `runs stats`, already
   recorded by class (`docs/results/e-submission.md`, "Rejections, causes and recoveries"):
   Haiku's 14 invalid submissions under v1 and 4 under v2 by the rule each broke, and its
   first rejections by class. The counts go into ADR 0042's amendment; the row closes with
   the fix below or a line.
2. **v3.** v1's list of items, each with its checklist id (no keyed object: Flash faulted 14
   times under v2's); v2's one repair naming every problem, and its recoveries; and one new
   recovery, `cites_dropped`: a finding whose citations name a call the run does not hold is
   read as the finding without those citations, recorded, instead of a rejection. Every other
   rule of v1 holds. `submission = "v3"` on `check`, the recipes and a matrix arm, as v2's
   setting is (ADR 0042, item 1); the default stays `v1`.
3. **Tests**: a provider conformance fixture of v3 for each adapter; the recoveries counted
   in `rejections/2`; v1 byte for byte unchanged.
   `submission::v3_drops_a_citation_the_run_does_not_hold_and_records_it`;
   `submission::v1_is_unchanged`.
4. **ADR 0042 amended**: v3, and why it is shaped by the classes counted.

- [ ] Done when: the tests pass. **Reviewer reads hardest:** that dropping a citation can never
  turn a finding into a pass, only into an uncited finding the verdict rules read as they read
  any.

## D1. The new set versions and the fresh set (the orchestrator's, with the owner)

1. **New versions of the source-fidelity dev sets.** `source-fidelity/scenes-v2` and
   `source-fidelity/adversarial-v2`: the same items, families and pictures as v1, jobs written
   again with full requirements by B1 from plans whose subjects state colour, count and
   lettering. A dev set's pictures may be viewed, so the building agent writes those
   attributes from them. Labels are carried from v1 and confirmed by the owner, who changes
   any a fuller requirement now decides otherwise. v1 is untouched; v2 is sealed beside it.
2. **The seasonal cover pair** (B1, item 8), generated from fresh images and committed as a
   dev fixture.
3. **The fresh set, `source-fidelity/confirm-v1`, and its plain twin.** 22 families of new
   scene kinds, disjoint from every family of every set, about 5 items each, about half
   expected to fail, among them edits of colour, of count and of lettering. Drawn by B1's
   seeded script into owner storage; built by `whizbang-sets build` with `--cap-usd 8`; the
   owner reviews each family's contact sheet and confirms or strikes each label; promoted with
   `--labels-out` to owner storage. Both twins stay whole in owner storage (manifest, jobs,
   pictures, labels) and are read by `--root` and `eval sets fetch --from` until R4 has run.
   **No agent opens a job, a picture, a contact sheet or a plan of it**, the orchestrator
   included: it runs the commands and reads what they print, which is counts.
4. **After R4**, the twins are published as `blind-v3` was: manifests and jobs into
   `evalsets/`, pictures by the owner into the pictures repository, labels kept in owner
   storage. From then on they are exposed.

- [ ] Done when: the v2 dev sets pass `eval sets check` and are merged; the fresh twins pass it
  under their root in owner storage, with the count of families and items printed and
  written into this plan; the generation's total is in "Spend". **Reviewer reads hardest:**
  that nothing of the fresh twins is in the pull request.

## R2. Rung 2 of the new tools and methods (the orchestrator's; nothing paid)

1. **The runs.** On the owner's Mac, on the GPU: `whizbang eval tool <tool> [--method <m>]
   --set source-fidelity/scenes-v1 --set source-fidelity/adversarial-v1`, for `outline` and
   `regions` under each method, and `classes` on the boxes `regions` gives. The same sets and
   script as `per_subject`'s rung 2.
2. **The bar, `rung2/1`, frozen here.** A method goes on to rung 3 when the family-resampled
   95% lower bound of its AUC is above 0.5 and it errs on at most a third of the items. For
   each tool, SAM 3's method goes on if it passes; otherwise the fixed-class method if it
   passes; otherwise the tool is **dropped** (its file removed, its record kept with the
   results). Where both pass, the cheaper in time goes on and the other is removed and kept in
   the record. `classes` is scored by the AUC of `class_difference` over the regions of
   failing against passing items.
3. **The records**: each tool's "Rung 2 results" and "Alternatives" (each method), and "Mark
   and history".

- [ ] Done when: each record holds its rung 2 and its outcome under `rung2/1`. **Reviewer reads
  hardest:** the rule applied as frozen; aggregates only.

## R3. Rung 3 of squint's tools on `blind-v3` (the orchestrator's)

1. **The gate files, frozen before the run** under `gates/e-rung3-<tool>.toml`, one per
   treatment, all on one matrix `gates/e-rung3-matrix.toml`: `source-fidelity/blind-v3`, arm C
   and each treatment, `claude-haiku-5-5`, one repetition. Treatments, in the order of the
   ladder: `C;second_look=on@source-fidelity`, `C+classes` (with `regions`), `C+edge_similarity`,
   `C+change_map`, `C+regions`, `C+per_subject`, and `C+outline` if it passed R2. A tool R2
   dropped is not run. The spec's defaults (section 8, "Gates"), false accepts as a guardrail
   too, families at least 20 (the set has 24), Bonferroni over every treatment of the matrix.
2. **The run**, in the evaluation store, `blind-v3`'s labels by `--labels <owner storage>`.
3. **What a receipt decides.** Admitted at rung 3 means the tool goes on to R4; it admits
   nothing (A0, item 4). Rejected or inconclusive: the tool ends E **experimental**, or
   **dropped** if the upper bound of its gain is below zero (it made the judge worse with
   confidence). Recorded either way, in its record and in `docs/results/e-rung3.md`.
4. **The cap** of one call: the bound of one check by Haiku under the dearest treatment, plus
   a cent, from the binary that runs the round.

- [ ] Done when: the receipts and the record are committed, held by
  `evidence::the_rung3_record_holds_only_aggregates`. **Reviewer reads hardest:** that no blind
  label or item is in the record.

## F2. The dev round of the format and the requirements (the orchestrator's)

1. **The matrix**, `matrices/e-format-dev.toml`: the ten dev sets, arms `C;submission=v1`,
   `C;submission=v2` and `C;submission=v3`, judges `claude-haiku-5-5` and `gemini-3.8-flash`,
   one repetition. With them, `source-fidelity/scenes-v2` and `adversarial-v2` under
   `C;submission=v1`, for the requirement change on dev data.
2. **The rule, `submission/2`, frozen here**: `submission/1`'s clauses (a) and (b), and clause
   (c) read on the family-resampled 95% intervals: no judge's balanced accuracy falls, nor its
   false-reject or false-accept rate rises, by more than 2 points at the interval's bound
   toward harm. Of v2 and v3, the one that meets the rule with fewer submission failures goes
   to R4; if neither does, the one with fewer failures goes to R4 all the same, since only a
   gate on fresh data admits, and the record says so.
3. **The record**, `docs/results/e-format-dev.md`: descriptive, admits nothing; the
   requirement versions' figures beside v1's, said to be on dev data.

- [ ] Done when: the record is committed and held by an evidence test. **Reviewer reads
  hardest:** the rule applied as frozen.

## R4. Rung 4: the confirmation round (the orchestrator's)

1. **Asked first.** The fresh set is held out: the orchestrator asks the owner before it is
   used ("I need", item 4), and states the hypotheses below in the ask.
2. **The matrix and the gates, frozen before the run**, committed under `gates/e-confirm-*`:
   one matrix on `confirm-v1` and `confirm-plain-v1`, one repetition:
   - arm C under all five judges (the judge's gate, and every gate's control);
   - the format F2 sent (`C;submission=v2` or `v3`) under Haiku, Flash, Terra and Argon;
   - each tool that passed R3, `C+<tool>`, under Haiku and Terra;
   - `confirm-plain-v1` under arm C, under Haiku and Terra (the requirement gate's control,
     a gate across two versions of a set, M1, item 2).
   Bonferroni over every hypothesis of the round: two for each tool, four for the format,
   two for the requirements.
3. **The judge's gate**: `judge-pick/1` on `confirm-v1` under arm C, frozen as a rule file
   beside the gates. The pick is admitted as the default judge, and source fidelity with it
   (A0, item 4); no pick leaves both unverified. ADR 0044 is amended by the outcome.
4. **The run**, in a new store, the confirmation store, whose allowance the owner sets ("I
   need", item 1); sets by `--root <owner storage>`, pictures by `eval sets fetch --from
   <owner storage>`, labels by `--labels <owner storage>`. The cap of one call: the largest
   one-check bound among the five judges under the dearest arm, plus a cent.
5. **What each receipt decides.** A tool, the format or the requirements: admitted when every
   judge's comparison is; then its mark becomes admitted, and, for the format and the
   requirements, the default changes (the `submission` default in `check` and the five recipe
   files; the builder's default wording). Rejected or inconclusive: the tool ends E
   experimental, or dropped by R3's rule. If the format is admitted and the default judge's
   comparison under it is among them, the default judge stays the pick of item 3.
6. **The records**: `docs/results/e-confirm.md` and its TOML; each record's "Mark and history";
   `docs/guides/config.md` and `docs/guides/loop.md` where a default changed. Held by
   `evidence::the_confirmation_record_holds_only_aggregates`. Then D1, item 4.
7. **Tier 2, only on the owner's yes** ("I need", item 2): the study's one comparison that
   excluded zero, `regions` on `gemini-3.8-flash`, repeated with two repetitions of each of C
   and `C+regions` on `confirm-v1` (the carry-over row on the study's one repetition), in the
   same matrix before it is frozen. Without the yes, N1 withdraws the claim with a line in
   `docs/results/2026-11-tool-study.md` and the row.

- [ ] Done when: every receipt is committed, `whizbang eval admitted` lists what they admitted,
  and the records hold. **Reviewer reads hardest:** that nothing of the fresh set is
  committed before item 6; that the gates were frozen before the run (their commit precedes
  the first cell's time).

## P0. The pairs page (small)

1. **Sizes that follow from dimensions alone** (the carry-over row on the padding chunk). Each
   picture of a pair is written as an uncompressed PNG (stored deflate blocks), so its file
   size and every chunk length follow from its width and height; the padding chunk goes.
   `pairs_page::a_pictures_bytes_tell_only_its_dimensions`.
2. **The engine its test needs** (the carry-over row on `node`). Node is pinned among the
   tools `just check` needs since milestone 5; `pairs_page::a_backspace_takes_back_the_last_pick`
   now fails where it is missing, instead of holding the page to its text.
3. **`not_worse`** (the carry-over row on the picking rounds, its last clause). Under the
   frozen protocols it cannot be reached; a protocol is not changed after it was frozen, so
   their decodes report `preferred`, `inconclusive` or `worse`, and the record says why.
   E's new protocols (R5, R6, W1) state `not_worse` as "losses at most a third of the decided
   pairs, and wins under two thirds", which can be reached, with a test of the decoder.

- [ ] Done when: the tests pass. **Reviewer reads hardest:** that an old page's picks still
  decode against its frozen assignment.

## V1. Visual quality (the orchestrator's, with the owner)

1. **The picking rounds of milestone 3** (the carry-over row on them). The pages are drawn
   again from the kept stores with P0's writer (`whizbang lab pairs`): `m3-seam`, 30 pairs,
   and the four `m3-edits-<recipe>` labs, 24 pairs each, written to owner storage. The owner
   picks them. `whizbang lab decode` holds the picks to each frozen assignment and decodes
   them. If a store is gone, the picks on the page already drawn are decoded instead.
2. **The loop's contact sheet** (the carry-over row on the loop's retries). The owner marks
   it; `whizbang eval loop score --labels <owner storage>` adds whether the named thing is
   still visible and whether something else was damaged, beside the attempt before.
3. **The records.** Each recipe's "Rung 2 results" names its decode, its visual quality is
   shown or not, and its mark is set by section 8: rungs 1 and 2 shown makes it unverified;
   the seam's beside rascal's `seam-03` and `objects-02`. `fill`'s rung 2 is credited from
   these decodes and `docs/results/m3-edits.md`'s preservation, Bria's through fal in all.
   `docs/results/e-loop.md` gains the marks' tables. Aggregates only.

- [ ] Done when: the decodes are recorded and the marks set, or the owner has said the rounds
  are not wanted. Nothing paid. **Reviewer reads hardest:** that the decode used the frozen
  assignment.

## L1. The loop's record and evidence (medium)

1. **A closed set of reasons** (the carry-over row on `runs stats`): a loop's record says why
   its attempts ended as one of a closed set of reasons beside the sentence, by ADR 0005's
   rule, and `runs stats` counts by it. Older records count by their sentence, as now.
2. **Located retries that added nothing** (the carry-over row on them): the tally counts how
   often a located retry added no pixel, and how often the widening after it passed.
3. **The tenth** (the carry-over row on `ITSELF_PART`): from the tallies of plan e1's loop
   round (`docs/results/e-loop-<judge>-<n>.toml`, `[places]`), the places that joined as
   themselves and those left out over a tenth, with what the checks after them said; the
   number is kept or changed with a line in the row, by the rule: changed only if more than
   a quarter of the places looked into were left out for size.

- [ ] Done when: the tests pass and each row has its line. Nothing paid.

## L2. The located retry, whole (medium to large)

1. **The phrase delimited** (the carry-over row on `JOB_TEMPLATE`): the caller's phrase is
   marked where it starts and ends in `RQ1` and the brief; the template has a new version and
   digest; a test holds a phrase that reads as an instruction to its marks.
2. **A place of any size** (the carry-over row on `located::usable`): a place whose cut passes
   1,024 pixels a side is looked into in tiles, each carried as the picture's own pixels; a
   polygon joins as its own pixels.
3. **One mask at a time** (the carry-over row on `select`'s memory): `select` joins masks
   holding one and the union.
4. **One decode** (the carry-over row on `carry` and `view`): a retry decodes its picture
   once; `carry`'s comparison looks at the run's signal row by row; the times of a located
   retry on a large picture are in the loop's record.
5. **A carried mask reaching a fill** (the carry-over row on the model tier's fixture): a
   fixture with a second thing the first find misses, on which the located retry's mask is
   filled and checked, in the model tier.
6. **The segmenter's threshold inside a crop** (the "Not ported" row on the located retry):
   none is added; plan e1's tallies show what crops found, and the line says so.

- [ ] Done when: the tests pass; `remove_checked`'s record is updated and stays experimental
  until R5. Nothing paid.

## C1. The clicked removal (medium if built)

1. **Measured first, nothing paid** (the carry-over row on a clicked removal). On the 20
   pictures of `segmentation/segment-01`, SAM 3 on the Mac: a point request at a point inside
   each truth mask, one instance, its mask unshaped, against the same mask grown, closed and
   feathered as `remove` shapes a location's. Mask coverage of the truth mask, as
   `eval loop score` reads it (plan e1's measure, under-covered below 0.95).
2. **The rule, frozen here**: the recipe is built when the shaped mask is under-covered on at
   least 3 fewer pictures than the unshaped one. Otherwise the row closes with the counts.
3. **If built**: a recipe that removes a thing from a mask a caller gives, shaped as `remove`
   shapes a location's, then filled; its record (rung 1 examples, marked experimental); the
   playground's clicked removal calls it. It climbs with the others in R5.

- [ ] Done when: the counts are recorded and the recipe built or the row closed.

## P1. Panoramas (large, two pull requests)

**First pull request: `view`'s panorama side.** Ported from squint's
`crates/squint-tools/src/builtin/view.rs` and `raster.rs` at the pinned commit (the carry-over
row on the panorama side of `view`):

1. The square perspective view by yaw, pitch and field of view, the bilinear sample that wraps
   across the seam, the refusals for angles, and the crop of a panorama by box; the refusal
   of an `also` of another projection than `image`; squint's stored-view and analytic tests.
2. The perspective view's mapping as a kind in `binding/1`, with its own ADR (ADR 0001's
   mappings did not include it).
3. `view`'s record: the limit on panoramas removed; rungs 1 and 2 cover them.

**Second pull request: locations, the slack, alpha and the seam's records.**

4. **Locations through every binding** (the carry-over row on `frames/`): polygons and views
   as locations, their validity rules, footprints cut at the seam, locations through a
   rotation and through the perspective view.
5. **The slack scaled** (the carry-over row on `land`): the slack scaled to the mapping, with
   a test on a heavily magnified crop of a crop.
6. **A reduced crop with transparency** (the carry-over row on it): the three ways compared on
   synthetic examples in `view`'s record (the mean as it is, weighted by alpha, flattened onto
   a ground); the default kept or changed by the rule that a hidden colour must not show at a
   visible edge.
7. **The seam's records** (the two carry-over rows "with the seam's rung 3"): a floor on
   `fan`'s and `ramp`'s windows, or a line that the scale is meant, with an example on a small
   panorama; `measure`'s continuity given its unit and range with a vector.
8. **Squint's spherical estimator of `align`** (the carry-over row left by task A2): E builds
   no set of panoramas to align, since the seam's check compares an edit with its source in
   place; re-owned with a line to the first such set, no milestone.

- [ ] Done when: both pull requests are merged and each row has its line. Nothing paid.

## R5. Rung 3 of the edit recipes (the orchestrator's)

1. **The sample.** `remove`, `remove_prompt`, `remove_checked` and the clicked removal (if C1
   built it) on 10 pictures of `segmentation/segment-01`, two from each group and two more
   from the groups e1's loop round found hardest; `repair` and `repair_area` on 10 pictures of
   `labs/m3-edits/corpus.toml`; `seam` on 10 panoramas of `labs/m3-seam/corpus.toml`. In the
   order of the ladder: `remove_checked`, `seam`, `repair_area`, `repair`, `remove_prompt`,
   `remove`.
2. **The gate, frozen before the run**, one a recipe, `gates/e-edit-rung3-<recipe>.toml`: task
   success is the share of finished edits `check` under the default judge passes, at least
   0.7 with a Wilson 95% lower bound above 0.5; preservation outside the mask exact; an
   operational error rate under 10%. Each recipe on its own; too few runs is inconclusive.
3. **The run**, in the loop store, SAM 3 on the Mac and Bria through fal; the cap of one call
   is `remove_checked`'s three-attempt bound under the default judge, plus a cent.
4. **What it decides.** Passed: the recipe goes on to R6. Not passed: it ends E
   experimental, recorded, unless its rung 2 decode was `worse`, when it is fixed and tried
   once more or dropped by the orchestrator's ruling, said in the record.

- [ ] Done when: `docs/results/e-edit-rung3.md` and the gate receipts are committed, held by an
  evidence test.

## R6. Rung 4 of the edit recipes on the owner's photos (the orchestrator's, with the owner)

1. **The corpus**: 12 of the owner's own photos, each with the phrase of what to remove, where
   one pass may leave something behind (the carry-over row on the owner's photos). In owner
   storage; never committed.
2. **The protocol, frozen before the run**: each recipe that passed R5 against the source
   picture as reference arm, `whizbang lab` blinded pairs, P0's reachable `not_worse`, at least
   10 decided pairs a recipe, the owner's picks as labels. Section 8: the labels are not
   squint's.
3. **The run** in the loop store; the owner picks the pages; `lab decode` decides.
4. **What it decides.** `preferred` or `not_worse`: admitted, the mark admitted with the
   receipt. Otherwise experimental. Recorded in `docs/results/e-edit-rung4.md`, aggregates
   only.

- [ ] Done when: the decode is recorded, or the owner has said the photos are not wanted, when
  the row closes with that line and the recipes end E unverified or experimental as V1 and R5
  left them.

## W1. What milestone 8 builds (medium; the orchestrator's measure)

1. **The segmenters compared**, nothing paid: SAM 3, Tiny and the fixed-class segmenter (on the
   phrases its list holds) on the 20 truth masks of `segmentation/segment-01`, by mask
   coverage and IoU, by group. **The rule, frozen here**: milestone 8 builds a further
   segmenter only if SAM 3 is under-covered on at least a quarter of a group's pictures and
   another model in the catalogue or a named candidate is not. Otherwise no further
   segmenter. `segment`'s rung 2 is shown by this table (mask agreement with references) and
   it becomes unverified.
2. **How local fill is scored in milestone 8**: a lab protocol frozen and committed under
   `labs/m8-fill/`: local fill against Bria through fal on the same masks of segment-01 and the
   m3-edits corpus, blinded pairs, `not_worse` as P0 states it, task success and preservation
   as R5 scores them.
3. **How a local judge is scored in milestone 8**: `judge-pick/1`'s eligibility on the ten dev
   sets, then a gate on a fresh set milestone 8 builds with B1's builder; the set's size and
   rule stated here. Not `confirm-v1`, which R4 exposes.
4. **An ADR**, "What milestone 8 builds", with the three outcomes.

- [ ] Done when: the ADR, the record of item 1 and the protocol files are committed.

## N1. The note

1. **The note**, `docs/plans/mE-note.md`, in milestone 7's shape: the row's clauses each with
   its evidence (held by `scripts/tests/test-notes.sh`); a table of every tool and recipe with
   its end state and the receipt that set it; spend by round against this plan; the timing
   report; what moved to the carry-over list; what a reader should not assume (a gate on 22
   families is small; an exposed set admits nothing).
2. **The spec's wording owed**: section 7's squint row on `regions`, `classes` and `outline`
   and the segmenter they use, and rascal's rows, marked as E left them; section 4's
   segmenters naming the fixed-class entry; section 16, row E, if the owner's answer to "I
   need", item 3 changed it.
3. **The study's claim**, withdrawn with a line if tier 2 did not run.
4. **The tag `mE`**, by the orchestrator, on the note's merge.

- [ ] Done when: the note is merged and `mE` is tagged.

## Carry-over rows owned by milestone E, and who takes each

| Row (by its subject in `docs/carry-over.md`) | What happens to it |
|---|---|
| The panorama side of `view` | P1, items 1 to 3 |
| A reduced crop with transparency averages colours without alpha | P1, item 6 |
| The rest of what `frames/` holds | P1, items 2 and 4 |
| The fixed slack in `land` | P1, item 5 |
| `resize`'s methods (dropped) and squint's spherical estimator of `align` | P1, item 8: re-owned to the first panorama set to align |
| Job requirements omit colour, count and lettering | B1, item 5; D1, items 1 and 3; R4 |
| A test pair for a seasonal cover | B1, item 8; D1, item 2 |
| Small models break the submission format | F1, F2, R4 |
| What the port of the use cases left of the rules | T1, item 4 |
| Which tools a gate admitted | M1, item 3 |
| The 2026-11 study's one repetition | R4, item 7 (tier 2), or withdrawn by N1 |
| The study's results by kind of failure | The owner's answer ("I need", item 5); if yes, `scripts/baseline/tables.py` adds the table in N1 |
| A clicked removal is filled as `segment` found its mask | C1 |
| The Google rows of prices | A0, item 1, at the first Google round on or after each date |
| A resumed matrix never runs a failed cell again | M1, item 4 |
| `fan` and `ramp`'s windows; `measure`'s continuity | P1, item 7 |
| The pairs page's script and `node`; the padding chunk | P0, items 1 and 2 |
| The blinded picking rounds of milestone 3 | P0, item 3; V1, item 1 |
| Haiku's invalid submissions | F1, item 1 |
| `runs stats` counts loops by sentence | L1, item 1 |
| The loop's job puts the phrase at the end | L2, item 1 |
| The located retry does not look into every place | L2, item 2 |
| A located retry that adds no pixel | L1, item 2 |
| `ITSELF_PART`, the tenth | L1, item 3 |
| `select` holds every mask at once | L2, item 3 |
| `carry` and `view` decode the picture every call | L2, item 4 |
| The model tier's located retry never reaches a fill | L2, item 5 |
| The loop's retries have no live evidence (the contact sheet) | V1, item 2 |
| The judge round's figures copied by hand | M1, item 5 |
| The default judge and the format admitted by no gate | R4, items 2 and 3 |
| The owner's own photos | R6 |
| The pictures' repository's old name | B1, item 6 |
| "Evaluations": per-subject comparison; inherited tools; `regions`, `classes`, `outline`; a fresh confirmation set; `tiles`, the second look and details first; the top Google model again | R2 to R4 and D1; `tiles` ends experimental; details first A0, item 5; Argon is run again under arm C in R4 |
| "Evaluations": segmentation that labels every pixel | T2, item 2 |
| "Deferred until evaluated": the three tools, the fixed-class segmenter | T1, T2, then N1 moves them out of the section |
| "Not ported": the segmenter's threshold inside a crop | L2, item 6 |

Opened by this plan, owned by a later milestone: the fresh sets of the four other use cases,
if the owner says yes to "I need", item 3; a fresh set for milestone 8's local judge (W1,
item 3).

## Documentation that changes

- New ADRs: "Gates of milestone E" (M1), "The fixed-class segmenter" (T2), "The perspective
  view" (P1), "What milestone 8 builds" (W1), and one for the set builder (B1), numbered as
  they are written. Amended: ADR 0042 (F1, R4), ADR 0044 (R4), ADR 0026 (A0, item 1).
- `docs/evalsets.md`: building a set (B1), the new versions and the fresh twins (D1), several
  gates on one matrix (M1).
- `docs/reference/tools/`: new records for `outline`, `regions`, `classes` and the clicked
  removal if built; every tool's "Mark and history" as the ladder leaves it; `view`'s
  panorama side.
- `docs/guides/config.md`, `docs/guides/loop.md`: defaults that change (R4).
- `docs/results/`: `e-rung3`, `e-format-dev`, `e-confirm`, `e-edit-rung3`, `e-edit-rung4`, the
  segmenters' comparison, each with its TOML and evidence test.
- `SPEC.md`: A0, item 4; N1, item 2. `docs/carry-over.md`: each task's rows, then N1.
- `docs/reference/`: regenerated whenever the description changes (A0, T1, T2, F1, C1).

## Commit strategy

- One task is one branch and one pull request (two where the table says), tests first,
  pushed once after one full `just check` on the finished tree, reviewed whole, merged by the
  orchestrator with a merge commit once `ci` and both scans pass (the sixteen jobs).
- An ADR is in the pull request of the task that names it, its first commit.
- A round's gate files and matrix are committed and merged before the round starts; its
  receipts and records come in a second pull request of the same task.
- Every state of `main` between tasks passes `just check`, and `check` answers as at 0985bfd9
  until R4 changes a default on a receipt.
- **Rollback checkpoints.** The tag `m7` (exists). `mE-tools`, when A0, M1, B1, T1, T2 and F1
  have landed: the code, no default changed, no round run. `mE-squint`, on R4's records'
  merge. `mE`, on N1's merge. A pushed tag is never moved.
- **If a task shows a decision here is wrong,** this file is changed first, in its own pull
  request; a change of scope gets review rounds first. A frozen rule is never changed after
  its round started; a new rule is a new round.

| Tasks | To undo |
|---|---|
| A0 | Revert the merge: `describe` shows nine marks again |
| M1 | Revert the merge: gate files of version 3 are refused again |
| B1 | Revert the merge: no set is changed by it |
| T1, T2 | Revert the merges: the tool files and the entry go; their records stay as history |
| F1 | Revert the merge: v3 is a setting no default uses before R4 |
| R4 | If it changed a default, revert its records' merge or set the setting per call; the receipts stay |
| P0, L1, L2, C1, P1 | Revert the merge; each changes no default but the loop's template (L2), whose old version is still read |
| Rounds (R2, R3, F2, V1, R5, R6, W1) | Records and receipts |

## Spend

All on approved recipients, run by the orchestrator on the owner's Mac. The rates are
`docs/results/e-judges.toml`'s cost per decision on source fidelity under arm C: Haiku
0.00357, Terra 0.01106, Flash 0.01842 (double from 2026-12-31), Sol 0.02238, Argon 0.02976
(double from 2026-11-30, so 0.0595 for every round of E). A tool's arm is taken at 1.5 times
its control. Bria through fal at 0.04 a fill. These are estimates, not limits: the limits are
each store's allowance, each call's cap and the builder's `--cap-usd`.

**What E has left: 63.88 dollars** of the milestone's 100, since the first slice settled 36.12.

**Tier 1, within what is left.**

| Task | What is paid | Estimate (dollars) |
|---|---|---|
| A0, M1, B1, T1, T2, F1, P0, L1, L2, C1, P1 | Nothing: fakes in every test; SAM 3 and the fixed-class segmenter on the Mac | 0 |
| R2, V1, W1 | Nothing: local models and the owner's picks | 0 |
| D1 | About 70 pictures generated for 22 families and the seasonal pair, under `--cap-usd 8` | 6 |
| R3 | 143 items: Haiku under C, and up to seven treatments | 5 |
| F2 | 110 dev items under three formats and 44 under the new versions, Haiku and Flash | 7 |
| R4 | 110 items: C under five judges 12.7; the format under four 10.2; up to three tools under two 7.3; the plain twin under two 1.6 | 32 |
| R5 | 60 runs of a recipe, a fill and a check by Argon each, `remove_checked` up to three | 5 |
| R6 | 12 photos under up to six recipes | 3 |
| | **Expected** | **about 58; about 62 if Flash's price has doubled by R4** |

**Cap: 63 dollars settled**, counted across every store and the builder. A round that would
take the milestone past it does not start; a round that passes twice its estimate stops and
is said. There is no allowance for a round run again after a fix: such a run is tier 2.

**Tier 2, only on the owner's yes** ("I need", item 2), about 40 more:

| What | Estimate (dollars) |
|---|---|
| The study's comparison repeated: `regions` on Flash, two repetitions of C and of `C+regions` on `confirm-v1` | 12 (21 after Flash's price doubles) |
| Flash beside Haiku and Terra in each tool's gate on fresh data | 6 |
| Sol under the new format, so every judge is read under it | 3 |
| One round run again after a fix | 10 |

**E needs more than it has left.** Tier 1 spends nearly all of the 63.88 and leaves nothing
for a round run again. Tier 2 lets the study's one claim be tested rather than withdrawn,
and reads every judge under a new format. So this plan asks the owner to raise E's ceiling to
140 dollars in all ("I need", item 2), and plans nothing past 100 without that yes.

**Stores.** R3 and F2 run in the evaluation store (about 40.94 left): their forecasts, at the
study's rates doubled, are about 9 and 12. R4 needs a store of its own: its forecast is about
60, more than any store has left, so the confirmation store's first allowance, 75 dollars, is
a reset and the owner's ("I need", item 1). The allowance is a guard; what counts is settled.
R5 and R6 run in the loop store (about 17.71 left).

## Dependencies

```
A0 ──────────────────────────────┐
M1 ─────────────────────┐        │
T1 ──► T2 ──► R2 ──► R3 ┼──► R4 ─┼──► W1 ──► N1
B1 ──► D1 ──────────────┤        │
F1 ─────────► F2 ───────┘        │
P0 ──► V1 ───────────┐           │
L1 ──► L2 ───────────┼──► R5 ──► R6
C1 ──────────────────┤
P1 ──────────────────┘
```

- T2 after T1: its methods extend T1's tools.
- R2 after T2: both methods of each tool are scored together.
- R3 after M1 and R2: several gates on one matrix, and only tools that passed rung 2.
- F2 after F1 and D1's dev versions.
- R4 after D1's fresh twins, R3 and F2: only tools that passed rung 3, the format F2 sent, and
  the owner's yes to use the fresh set.
- R5 after V1 (rung 2 done first), L2 (the loop as it will stay), C1 and P1 (the seam's check
  can look into a panorama).
- W1 after T2 (its model is compared), R4 (the fresh set is spent) and R6.

## Risks

1. **The gates may admit little.** Twenty-two families and one repetition give wide intervals;
   a gain of 3 points with a corrected lower bound above zero is hard to show, most of all for
   judges already near 95%. A tool that cannot show it ends experimental, which row E allows;
   the record says "inconclusive" plainly and does not call it a failure.
2. **Haiku sends sound pictures to review.** Its `unsure` counts as neither a catch nor a pass,
   so a tool that only shifts its unsure rate shows little on balanced accuracy. The false
   accepts guardrail and the decided-correctly figure beside each receipt say what happened.
3. **The budget is nearly spent by tier 1.** A rerun has no room without the owner's yes; a
   price change can push R4 past the cap, when the orchestrator drops treatments in reverse
   order of the ladder (least doubtful first) before it runs, and says so.
4. **A fresh set built by generators** may carry the generators' habits; families are drawn
   from new scene kinds and three generators to spread them. The owner's review strikes a bad
   item; a family that loses all its items is struck, and fewer than 20 families makes R4
   inconclusive by rule, so D1 builds 22.
5. **The fixed-class segmenter is large** (about 1.2 GB in squint's export) and slow on CPU; it
   runs on the Mac's GPU. If it cannot run there through `transformers`, T2 stops and the
   three tools are scored on SAM 3 alone, the catalogue entry recorded as not working.
6. **v3 may still fail small models**, or cost accuracy. Only R4 can admit it; F2 sends the
   better of v2 and v3, and v1 stays the default otherwise.
7. **Owner time.** Contact sheets for 22 families and two dev versions, about 126 pairs to pick,
   a loop contact sheet and 12 photos. Each is optional to the code; each round that needs one
   waits for it, and the plan says which rows close without it.
8. **Owner-only material in a record.** Every record is written from a TOML by a command that
   writes aggregates, and held by an evidence test; the reviewer reads each for an item, a
   label or a path.
9. **The fast tier's time.** Each task reports its share; 15 seconds in all.

## What the owner provides

- **D1**: review of each family's contact sheet and each label of the two dev versions and the
  fresh twins (about 140 items).
- **V1**: the picks of the five pages (about 126 pairs) and the marks of the loop's contact
  sheet.
- **R6**: 12 of your own photos, each with the phrase of what to remove, in owner storage, and
  the picks of R6's pages.
- **T2, only if needed**: a yes to a new origin for the segmenter's files, if they are not
  served from `https://huggingface.co`.

## What the owner is told, not asked

- Twenty-two tasks, eight of them the orchestrator's rounds; squint's ladder first, then
  rascal's.
- One fresh set, for source fidelity, kept whole in owner storage until its one round, seen
  by no agent; published after it, as `blind-v3` was.
- The format is redesigned once more (v3), and v2 and v3 meet on dev data before the better
  goes to the gate.
- The fixed-class segmenter comes back as a catalogue entry for `classes`; SAM 3 stays the
  default.
- `align` loses its mark: it is a control, complete with rungs 1 and 2.
- Details first, `resize` and `tiles` end E experimental.
- Expected spend about 58 dollars of the 63.88 E has left; cap 63.

## I need

1. **I need your yes to set the confirmation store's allowance to 75 dollars** (yes/no). It is
   a reset, which is yours (section 6); R4's forecast doubles its expected 32. Without it R4
   cannot start.
2. **I need a yes or no: may milestone E spend up to 140 dollars in all, instead of 100?** The
   first slice settled 36.12, tier 1 needs about 58 more, and tier 2 (the study's claim
   tested, every judge read under a new format, one rerun) about 40. Recommended: yes. With
   no, tier 2 does not run and the study's claim is withdrawn.
3. **I need a yes or no: may the four use cases other than source fidelity end E unverified,
   their fresh sets moved to a later milestone?** One fresh set costs about 6 dollars to
   generate, 25 to 35 to judge and about an hour of your review. Recommended: yes.
4. **I need your yes to use the fresh set once, in R4, for the hypotheses its gate files
   state** (yes/no). Section 15 asks first for a held-out set.
5. **I need a yes or no: may the study's results by kind of failure be published?** The kinds
   are your private grouping of `blind-v3`.

## Review rounds

None yet.
