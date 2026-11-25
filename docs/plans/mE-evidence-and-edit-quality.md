# Plan: milestone E, evidence and edit quality

Draft 3, 2026-11-25, written from `main` at 0985bfd9 (the tag `m7`, the merge of #312, the
note of milestone 7), after two outside review rounds ("Review rounds", at the end). Milestone E is `SPEC.md` section 16, row E, "Evidence and edit
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
   fresh sets, and asks the owner ("I need", item 3). The same holds for source fidelity if
   its own gate on fresh data is not cleared (R4, item 3): it stays the default, unverified,
   rather than taking `check` away. With the other four, `tiles`, which serves only jobs of
   one image, ends E experimental. Every other tool and recipe ends E admitted, experimental
   or dropped, by the rules of R2 to R6.
6. **The pictures' repository against sealed sets.** The carry-over row on the repository's
   old name asks that "each [set] names the repository's current name, the pins unchanged".
   Each set's `images_repo` is in its `set.toml`, which its seal covers, and sealed sets are
   never edited (section 8). Fix: B1, item 6: the new sets name the current name, and the
   reader takes the old name as an alias of it for sealed sets, with a test, instead of
   editing them.
7. **The retry of a failed cell.** The carry-over row on resumed matrices asks for "a rule
   for retrying a judge's failed cell". Section 9 already rules that a failed attempt "is
   reported, never replaced by a later success". Fix: M1, item 8: no retry, by that rule, in
   an ADR line; the row closes with it.
8. **A gate compares two arms of one matrix.** The code refuses a gate whose matrix has other
   arms (`crates/whizbang/src/gating.rs`: "the matrix's arms are not exactly the gate's
   control and treatment"), so every treatment pays again for its own control, and a gate
   cannot compare two versions of a set; a matrix runs every set, item, judge and arm
   together (`crates/whizbang-eval/src/matrix.rs`, `Matrix::cells`); arm C refuses the
   second look (`Arm::validate`); and `eval judges` passes over runs that carry a gate. Not a
   disagreement with the spec, but E's spend and design depend on them. Fix: M1, items 1 to
   6.
9. **A set builder.** Section 8 needs fresh sets and new set versions; whizbang has none of
   squint's set toolkit (squint `scripts/sets/`, `squint-sets build`, its `docs/evalsets.md`,
   "Building a set", at the commit `docs/sources.toml` pins), and the carry-over list neither
   ports nor drops it. Fix: B1 ports it.
10. **Admitted means on by default, but a gate measures some judges.** Section 8: admitted is
    "on by default"; a recipe is admitted as its combination of tools, order and settings
    (section 3, "Primitives, recipes and loops"), and changing it returns it to unverified. A
    tool's gate on fresh data measures it under the judges and format of its matrix, not under
    every judge a user may choose. Fix: A0, item 4 writes into section 8 that an admitted tool
    is on by default in the combination its gate measured, and one flag away under any other
    until a gate of that combination; R4, item 5 applies it.

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
   and each treatment beside it, and each gate reads its own two arms (M1, items 1 and 3). Every
   hypothesis of the round is counted in one Bonferroni correction, frozen before the run.
   The fresh set is used once, for the hypotheses its gate files state, and is exposed after.
4. **Rung 3 runs on `blind-v3` under a cheap judge.** It is the only set with 20 families
   that rung 4 does not need fresh: 143 items, 24 families, labels in owner storage. It is
   exposed, so a rung 3 gate decides only whether a tool goes on. Rung 2 stays on the
   source-fidelity dev sets, as `per_subject`'s did.
5. **Rung 4 judges are cheap and from two providers.** A tool's gate on fresh data is run
   under `claude-haiku-5-5` and `gpt-6.1-terra` ("improves judges across models"), and must be
   admitted for both. The format's gate runs all five judges, so that whichever judge the
   judge's gate picks was measured under it. The judge's gate runs all five under arm C.
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
8. **Edits climb on what exists, then on fresh cases of the owner's.** Rung 2 is finished by
   the owner's picks of the pages milestone 3 already drew (V1). Rung 3 runs each recipe on
   inputs of its own kind and scores it with `check` under the default judge (R5). Rung 4
   runs on fresh cases of each kind (photos with a phrase, photos with a stroke and an area,
   panoramas) and rests on the owner's blinded marks and picks, which squint does not produce
   (R6). Each edit gate states task success, preservation and blinded quality (M2).
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
| 11 | The default judge, and source fidelity | Unverified | Argon's lead comes from `blind-v3`, an exposed set; on dev sets alone it trails Flash | R4; unverified if its gate is not cleared ("I need", item 3) |
| 12 | The other four use cases | Unverified | No fresh set | End E unverified, if the owner agrees ("I need", item 3) |
| 13 | `remove_checked` | Experimental | Which attempt it keeps is not scored; 33 of Argon's 40 runs on segment-01 ended `fail` (`docs/results/e-loop.md`) | L1, L2, V1 (the contact sheet), R5, R6 |
| 14 | `seam` | Experimental | Its decode is owed; a judge cannot look into a panorama (`view` refuses any box on one); its protocol cannot reach `not_worse` | P0, P1, V1, R5 |
| 15 | `repair_area`, `repair`, `remove_prompt`, `remove` | Experimental | Visual quality owed (`docs/results/m3-edits.md`) | V1, R5, R6; a clicked removal beside `remove` (C1) |
| 16 | `segment` | Experimental | Rung 2 not shown | W1, item 1 (rung 2: mask agreement with segment-01's truth masks); rungs 3 and 4 credited from `remove_prompt`'s (R6, item 4) |
| 17 | `fill` | Experimental | Rung 2 not shown | Rung 2 credited from the recipes' once V1 decodes them (every fill there is Bria's through fal); rungs 3 and 4 from `remove_prompt`'s (R6, item 4) |

`view` and `align` are the control: complete with rungs 1 and 2 (A0, item 3; `view`'s
panorama side, P1).

## Acceptance: the row's clauses, each with its task and its evidence

| Clause | Task | Shown by |
|---|---|---|
| Its evaluations decide what 8 builds: a further segmenter | T2, W1 | W1's record and ADR: the comparison on segment-01, the rule frozen here, its outcome |
| ... and how local fill and judge are scored | W1 | The frozen protocol and gate files for milestone 8, committed, each named by its digest |
| Every unverified tool and recipe taken up the ladder, squint's first, the most doubtful first, until each is admitted, experimental or dropped | A0, R2, R3, R4, V1, R5, R6 | Each record's "Mark and history" names its end state and the receipt that set it; `describe` shows every mark (A0, item 3); `whizbang eval admitted` lists what the receipts admitted (M1, item 7); N1's table of every tool and its end |
| Per-subject comparison climbs rungs 3 and 4 on a fresh confirmation set | R3, D1, R4 | The rung 3 receipt on `blind-v3`; the rung 4 receipt on `confirm-v1` |
| `regions`, `classes`, `outline` brought in as experimental tools, scored on rungs 1 and 2, on SAM 3 where that works and on their old segmenter as a catalogue entry where not; those that pass go on | T1, T2, R2, R3, R4 | Each record's rung 1 examples, run as tests; R2's rung 2 table for each method; R3 and R4's receipts for those that pass |
| The submission format redesigned so small models can follow it | F1, F2, R4 | F2's record; the format's gate receipt on `confirm-v1` |
| Job requirements carry colour, count and lettering in a new set version | B1, D1, F2, R4 | `source-fidelity/scenes-v2` and `adversarial-v2` sealed beside v1; the requirement gate's receipt (`confirm-plain-v1` against `confirm-v1`) |
| Each change is admitted through its gate | M1, R4 | Every receipt committed under `gates/`; `whizbang eval admitted` |

## How the work is split

Twenty-four tasks. Sizes are changed lines of source, tests and hand-written documents, by
milestone 3's rate (about 2,500 for a large one). The orchestrator's rounds are named as
theirs; builders make no live call.

| Task | What | Builds on | Size |
|---|---|---|---|
| A0 | The entry check: the Google prices; E's ledger; every mark shown by `describe`, `align` as a control; the spec's wording for gaps 3 to 5; details first's end | `main` | Small: about 600 |
| M1 | The machinery of the squint rounds: a matrix that runs only the cells it names; the second look under C; gates frozen in their matrix, several on one; a gate across two versions of a set; `submission/2` and `judge-pick/2`; the list of what was admitted; no retry of a failed cell; the judge round's figures held to their TOML | `main` | Medium to large, two pull requests: about 1,300 (matrices and gates), then about 1,100 (the rules and the records) |
| M2 | The edits' gates: `edit-gate/1`, `eval edits gate`, marks for any recipe's pictures | `main` | Medium: about 1,200 |
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
| R5 | Rung 3 of the edit recipes (the orchestrator's) | M2, V1, L2, C1, P1 | Medium: about 600, gate files and records |
| K1 | The segmenters compared on segment-01's truth masks; `segment`'s rung 2; the rule for milestone 8 (the orchestrator's) | T2 | Small: about 400, a record |
| R6 | Rung 4 of the edit recipes on fresh cases (the orchestrator's, with the owner); `segment` and `fill` credited | R5, K1 | Small: about 400, records |
| W1 | What milestone 8 builds: K1's outcome; the protocols for local fill and judge | K1, R4, R6 | Medium: about 600, an ADR and protocol files |
| N1 | The note | all | Small: about 500, no code |

**Waves.** Each from `main` once what it builds on has landed.

1. **A0, M1, M2, B1, T1 and F1**, side by side. They share no source file but
   `docs/carry-over.md`. Beside them, the code tasks no squint round waits on: **P0, L1, C1**
   (its measurement waits for nothing paid) and **P1**.
2. **T2** when T1 has landed; **D1** when B1 has landed (the owner reviews the contact
   sheets); **L2** when L1 has landed.
3. **R2** and **K1** when T2 has landed; then **R3** (and M1); **F2** when F1 and D1's dev versions
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
4. **The spec's wording for gaps 3, 4, 5 and 10.** `SPEC.md` section 8 gains two sentences
   under "The ladder" (a rung 3 gate decides whether a tool goes on; only a rung 4 gate on
   fresh data admits), one under the table of kinds (a use case's rung 3 is a judge round
   under a rule frozen before it; its rung 4 is `judge-pick/2` on a fresh set of that use
   case), and one under "Marks" (an admitted tool is on by default in the recipe, judges and
   format its gate measured; under another judge or format it is one flag away until a gate
   of that combination). If the owner says yes to "I need", item 3, row E gains: "a use case
   of `check` whose gate on fresh data is not run or not cleared in E ends E unverified, the
   default it was". This is the spec brought in line with a reviewed plan.
5. **Details first's end.** Its record and `docs/reference/tools/source-fidelity.md` say it
   ends E experimental: no fix was proposed, and its gate rejected it in every run. `resize`,
   which `check` offers only under it, ends E experimental with it, its record saying so.
   The "Evaluations" row on `tiles`, the second look and details first is narrowed with that
   line.

- [ ] Done when: the two tests pass; `describe` shows every mark; the spec's sentences are in;
  the ledger's figures are in this plan. **Reviewer reads hardest:** that no record's mark
  changed but `align`'s.

## M1. The machinery of the squint rounds (medium to large, two pull requests)

**First pull request: matrices and gates.**

1. **A matrix that runs only the cells it names.** Today a matrix runs every set, item, judge,
   arm and repetition together (`crates/whizbang-eval/src/matrix.rs`, `Matrix::cells`), so
   R4's design would cost about five times its estimate. A judge's table may list the arms it
   runs (`arms = [...]`), and a set's table may list the arms and judges it runs; a cell
   outside both is not a cell. The forecast, the resume and the completeness checks count
   only the cells the matrix names. A matrix without these lists names every cell, as
   today, with the same identity. `matrix::a_judge_runs_only_its_arms`;
   `matrix::a_matrix_without_lists_keeps_its_cells_and_identity`.
2. **The second look under arm C.** `Arm::validate` refuses details and the second look on
   arm C ("C is the control"). The second look is not a detail measured first: it is one more
   turn after a submission that left an item tentative (`docs/results/m3-unsure.md`). Arm C
   may take `second_look`; details stay arm F's. `matrix::the_second_look_is_an_arm_of_c`.
3. **Gates frozen in their matrix, scored from its runs.** Gate file version 3: `[baseline]`
   names its control and treatment among the matrix's arms, or (item 4) two sets; the matrix
   may hold more arms. The binding runs one way only, so nothing is circular: a version 3
   gate names its matrix by file alone, with no `matrix_identity` (version 2's field, which
   hashes the matrix, is refused in version 3), and the matrix names, under `[gates]`, the
   digest of the bytes of every gate file frozen on it. The matrix's identity, which every run
   holds, then covers the gates, and proves they were frozen before its first cell. `whizbang eval matrix` runs the cells; `whizbang eval run --gate <file>` scores
   a gate from the runs of the matrix it names, reading only its own arms' or sets' cells,
   and refuses a gate whose digest the matrix does not name. `[confidence] hypotheses`
   counts every hypothesis of every gate the matrix names, the judge's rule included (M1,
   item 6), and a gate whose count differs is refused. Version 2 files read and score as
   before. `gate::a_gate_reads_its_two_arms_of_a_wider_matrix`;
   `gate::a_gate_the_matrix_does_not_name_is_refused`;
   `gate::gates_on_one_matrix_count_their_hypotheses_together`.
4. **A gate across two versions of a set.** `[baseline]` may name `control_set` and
   `treatment_set` instead of two arms: two sets of one use case whose items and families are
   the same by id, under one arm. Refused when the ids or families differ.
   `gate::a_gate_compares_two_versions_of_a_set_item_by_item`.

**Second pull request: the rules and the records.**

5. **`submission/2`** in `whizbang eval judges --mode arms` (F2): clause (c) read on the
   family-resampled 95% interval at its bound toward harm, the rest as `submission/1`.
   `judges::submission_2_reads_clause_c_on_the_interval`.
6. **`judge-pick/2`** in `--mode judges` (R4): `judge-pick/1`'s clauses, with a minimum of 20
   families below which the outcome is inconclusive, the interval at the level the matrix's
   correction gives it, and the runs read from a matrix whose cells `eval matrix` made, gates
   or not (today `eval judges` passes over runs that carry a gate, `judges.rs`). A test of
   too few families, of the corrected level, and of runs of a gated matrix.
7. **What was admitted** (the carry-over row on admissions). `whizbang eval admitted
   --receipts gates/` reads every receipt and prints each tool, use case, format, wording or
   judge a rung 4 gate admitted, with the receipt's digest and date; a receipt of a rung 3
   gate, or on a set whose split is not `blind` or `heldout`, admits nothing. Each record's
   "Mark and history" names its receipt by digest, and a test holds the records' marks to the
   list. `admitted::a_rung_3_receipt_admits_nothing`;
   `admitted::the_records_marks_are_the_receipts`.
8. **No retry of a failed cell** (the carry-over row on resumed matrices). A short ADR, "Gates
   of milestone E", records items 1 to 7 and: a resumed matrix or gate never runs a failed
   cell again, by section 9 ("a failed attempt is reported, never replaced by a later
   success"); a failure counts as an error, which the 2% guardrail sees. The row closes with
   that line.
9. **The judge round's figures held to their TOML** (the carry-over row on figures copied by
   hand). A test reads `docs/results/e-judges.toml` and requires each use case record's
   "Another judge" table and "Speed and cost" line, ADR 0044's figures and the two guides'
   figures to be the ones it gives.
   `evidence::the_judges_figures_outside_their_record_are_its_own`.
10. **A set read from owner storage before it is published.** `eval matrix`, `eval run
    --gate` and `eval judges` already take `--root` for the sets and `--labels` for blind
    labels; a test holds that a set under another root runs and scores, that a label folder at
    or below the sets' folder is refused, and that no record or receipt names the root's path.
    `gating::a_set_under_another_root_is_scored_and_its_path_is_nowhere`.

- [ ] Done when: both pull requests are merged and their tests pass. **Reviewer reads
  hardest:** that a version 2 gate and a matrix without lists score exactly as before; that a
  gate cannot read a cell outside its two arms or sets.

## M2. The edits' gates (medium)

R5 and R6 need a scorer that `eval run --gate` is not: it gates balanced accuracy of `check`.

1. **An edit gate file**, `edit-gate/1`, frozen before its round, stating every item section 8
   lists for a gate: the recipe and the corpus manifest, whose every case names its family (a
   scene; variants of one scene are one family); the measures, each with its bar: task success
   (a share of runs, with its 95% interval resampled by family against a bar), preservation
   outside the mask (exact: every pixel outside the kept mask's support is the picture's own,
   from the run's record), blinded quality (the outcome of a lab decode of this round's runs),
   and operational errors; the minimum number of families; the correction across the recipes
   of one round (Bonferroni on the intervals); the limits on cost (the settled cost of a run
   at most the recipe's guide figure doubled) and on latency (the median run at most twice its
   record's); and that too little data, too few families, or fewer picked pairs than the
   protocol's minimum is inconclusive.
2. **`whizbang eval edits gate --gate <file>`** scores it from the store: task success from
   the runs' checks (rung 3, R5) or from the owner's marks (rung 4, R6, read by `--labels
   <owner storage>` as `eval loop score --labels` reads marks), preservation from each run,
   quality from a decode file; it writes a receipt that `eval admitted` reads. Aggregates
   only.
3. **Marks for any recipe's finished pictures.** `eval loop sheet` and `eval loop score
   --labels` serve `remove_checked` today; they take any recipe's runs of a manifest, so the
   owner can mark `remove`, `repair` or `seam` as the loop's sheet is marked.
   `edits_gate::a_recipe_passes_only_with_every_measure`;
   `edits_gate::too_few_runs_is_inconclusive`; `loop_sheet::a_sheet_of_any_recipe`.

- [ ] Done when: the tests pass. **Reviewer reads hardest:** that a mark file is never read
  from inside the repo and that no mark or phrase reaches a receipt.

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
   is written into that subject's requirement as a checkable clause, in the form every tool
   that reads subjects already parses: the subject's phrase, then " keeps ", then what it
   keeps (`pack/tools/per_subject.py`, `KEEPS`). So "The red door keeps its identity, its
   colour (red), its count (two) and its lettering (OAK & ASH)", never "the two red doors
   keep". A plan flag `requirements = "plain"` writes today's wording, so one plan builds both
   twins from the same pictures. Tests hold both wordings, and a test runs `per_subject`'s
   `subjects_of` on a job of each twin and requires the same subjects from both, none of
   them not applicable.
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
   listed in `marks.rs`. Each tool declares the one number `eval tool` scores it by, as
   `per_subject` does: `outline`, one minus the overlap of the subject's two shapes, the
   largest over the job's subjects; `regions`, the score of its strongest region, zero when
   it finds none. Subjects are read by `per_subject`'s `subjects_of`, shared, not copied.
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
   marked experimental. The number `eval tool` scores it by is the share of the box whose
   dominant class differs between the two sides; `eval tool` gives a tool that takes `bbox`
   the whole frame as its box, so each item has one number and no tool feeds another.
5. **`method`** on `regions` and `outline` (section 8, "Comparing methods"): `sam3` (T1's,
   the default) or `eomt` (squint's class disagreement for `regions`; squint's class score
   for `outline`). Each method's examples run in the record.
6. **`whizbang eval tool --method <m>`**: the method passed to each run of the tool, and
   named in what it prints; refused for a tool that declares no methods. Today `eval tool`
   passes only the job (`crates/whizbang/src/trying.rs`). `cli::eval_tool_runs_a_method`;
   `cli::eval_tool_gives_a_tool_that_takes_a_box_the_whole_frame`.

- [ ] Done when: conformance passes with fakes; the model tier passes on the Mac; `classes`
  and both methods run under `eval tool --method` against the fakes. **Reviewer reads hardest:** that
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
   `regions` under each method, and `classes` on the whole frame (T2, item 4). The same sets
   and script as `per_subject`'s rung 2, each tool by the one number it declares (T1, item 3).
2. **The bar, `rung2/1`, frozen here.** A method goes on to rung 3 when the family-resampled
   95% lower bound of its AUC is above 0.5 and it errs on at most a third of the items. For
   each tool, SAM 3's method goes on if it passes; otherwise the fixed-class method if it
   passes; otherwise the tool is **dropped** (its file removed, its record kept with the
   results). Where both pass, the faster goes on and the other is removed and kept in the
   record.
3. **The records**: each tool's "Rung 2 results" and "Alternatives" (each method), and "Mark
   and history".

- [ ] Done when: each record holds its rung 2 and its outcome under `rung2/1`. **Reviewer reads
  hardest:** the rule applied as frozen; aggregates only.

## R3. Rung 3 of squint's tools on `blind-v3` (the orchestrator's)

1. **The matrix and its gates, frozen before the run**: `gates/e-rung3-matrix.toml`, naming the
   digest of each gate file `gates/e-rung3-<tool>.toml` (M1, item 3): `source-fidelity/blind-v3`,
   arm C and each treatment, `claude-haiku-5-5`, one repetition. Treatments, in the order of
   the ladder: `C+outline` and `C+classes` (each if it passed R2), the second look as a setting
   of arm C for source fidelity (M1, item 2), `C+edge_similarity`, `C+change_map`,
   `C+regions` (if it passed R2) and `C+per_subject`. The spec's defaults (section 8,
   "Gates"), false accepts as a guardrail too, families at least 20 (the set has 24),
   Bonferroni over every treatment of the matrix.
2. **The run**, in the evaluation store, `blind-v3`'s labels by `--labels <owner storage>`;
   then each gate scored by `eval run --gate`.
3. **What a receipt decides.** Admitted at rung 3 means the treatment goes on to R4; it admits
   nothing (A0, item 4). Rejected or inconclusive: the tool ends E **experimental**, or
   **dropped** if the upper bound of its gain is below zero (it made the judge worse with
   confidence). Recorded either way, in its record and in `docs/results/e-rung3.md`.
4. **The cap** of one call: the bound of one check by Haiku under the dearest treatment, plus
   a cent, from the binary that runs the round.

- [ ] Done when: the receipts and the record are committed, held by
  `evidence::the_rung3_record_holds_only_aggregates`. **Reviewer reads hardest:** that no blind
  label or item is in the record.

## F2. The dev round of the format and the requirements (the orchestrator's)

1. **Two matrices, two populations.** `matrices/e-format-dev.toml`: the ten dev sets, arms
   `C;submission=v1` and `C;submission=v3`, judges `claude-haiku-5-5` and `gemini-3.8-flash`,
   one repetition: every item under both arms, so the format is compared on one population.
   `matrices/e-requirements-dev.toml`, apart: `source-fidelity/scenes-v1` and `adversarial-v1`
   against `scenes-v2` and `adversarial-v2`, item by item, under `C;submission=v1` and the same
   two judges, scored as a comparison of two versions of a set (M1, item 4) and never mixed
   into the format's counts. v2 is not run again: plan e1's format round measured it against
   v1 on the same ten sets (`docs/results/e-submission.md`). If Flash's price has doubled
   before F2 runs, Flash leaves both matrices and Haiku alone runs them, said in the record.
2. **The rule, `submission/2`, frozen here** and applied by `eval judges` (M1, item 5):
   `submission/1`'s clauses (a) and (b), and clause (c) read on the family-resampled 95%
   intervals at their bound toward harm; every count and rate over the items scored under
   both arms of one judge alone. v3 goes to R4 if it meets the rule. If it does not,
   the format whose dev evidence is better by the rule's clauses goes, v2 by e1's round or v3
   by this one, said in the record; only a gate on fresh data admits either.
3. **The record**, `docs/results/e-format-dev.md`: descriptive, admits nothing; the
   requirement versions' figures beside v1's, said to be on dev data.

- [ ] Done when: the record is committed and held by an evidence test. **Reviewer reads
  hardest:** the rule applied as frozen.

## R4. Rung 4: the confirmation round (the orchestrator's)

1. **Asked first.** The fresh set is held out: the orchestrator asks the owner before it is
   used ("I need", item 4), and states the hypotheses below in the ask.
2. **The matrix and its gates, frozen before the run**, committed under `gates/e-confirm-*`,
   the matrix naming each gate's and the judge rule's digest (M1, item 3). One repetition.
   Only these cells run (M1, item 1):
   - `confirm-v1` under arm C, all five judges: the judge's gate, and every gate's control;
   - `confirm-v1` under the format F2 sent, all five judges, so that whichever judge item 3
     picks is measured under it;
   - `confirm-v1` under each treatment that passed R3, Haiku and Terra; in tier 1 at most two:
     per-subject comparison if it passed R3 (row E names its rungs 3 and 4 on fresh data), and
     the first other treatment in the order of the ladder that passed;
   - `confirm-plain-v1` under arm C, Haiku and Terra: the requirement gate's control, a gate
     across two versions of a set (M1, item 4).
   **The hypotheses**, counted together in one Bonferroni correction: two for each tool,
   five for the format, two for the requirements, one for the judge's rule, and, if tier 2
   runs, one for the study's comparison: at most 13 in tier 1.
3. **The judge's gate**: `judge-pick/2` (M1, item 6) on `confirm-v1` under arm C, at the
   round's corrected level, inconclusive below 20 families. The pick is admitted as the
   default judge, and source fidelity with it (A0, item 4); no pick, or inconclusive, leaves
   both unverified ("I need", item 3). ADR 0044 is amended by the outcome.
4. **The run**, in a new store, the confirmation store, whose allowance the owner sets ("I
   need", item 1); sets by `--root <owner storage>`, pictures by `eval sets fetch --from
   <owner storage>`, labels by `--labels <owner storage>`. The cap of one call: the largest
   one-check bound among the five judges under the dearest arm, plus a cent.
5. **What each receipt decides.**
   - **The format**: admitted for source fidelity when every judge's comparison is admitted;
     then the `submission` of source fidelity's recipe file changes, and only that one: the
     other four use cases were not confirmed under it and keep v1 in their files, which state
     it themselves (ADR 0042, item 1). Every judge was measured under it, so the default judge
     of item 3 was too.
   - **The requirements**: admitted when both judges' comparisons are; then B1's builder
     writes full requirements by default. It changes no recipe.
   - **A tool**: admitted when both judges' comparisons are, in the combination its gate
     measured: source fidelity under v1, Haiku or Terra (gap 10). It is on by default only
     there; under the default judge, or beside an admitted format, it stays one flag away, and
     a carry-over row is opened for the gate of that combination. Its record and `describe`
     say so. Source fidelity's default recipe therefore changes in E by the format alone, or
     not at all, and its record names the combination its receipt measured.
   - Rejected or inconclusive: a tool ends E experimental, or dropped by R3's rule; the
     format stays v1; the wording stays plain.
6. **The records**: `docs/results/e-confirm.md` and its TOML; each record's "Mark and history";
   `docs/guides/config.md` and `docs/guides/loop.md` where a default changed. Held by
   `evidence::the_confirmation_record_holds_only_aggregates`. Then D1, item 4.
7. **Tier 2, only on the owner's yes** ("I need", item 2), added to the matrix before it is
   frozen: the study's one comparison that excluded zero, `regions` on `gemini-3.8-flash`,
   with two repetitions of each of C and `C+regions` on `confirm-v1` (the carry-over row on the
   study's one repetition), if `regions` passed R2; Flash beside Haiku and Terra in each
   tool's gate; a third tool's arms. Without the yes, N1 withdraws the study's claim with a
   line in `docs/results/2026-11-tool-study.md` and the row.

- [ ] Done when: every receipt is committed, `whizbang eval admitted` lists what they admitted,
  and the records hold. **Reviewer reads hardest:** that nothing of the fresh set is
  committed before item 6; that every gate's digest is in the matrix every run holds.

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
   their decodes report `preferred`, `inconclusive` or `worse`, and the record says why. The
   decoder counts `same` and `neither` toward neither side (`crates/whizbang-eval/src/lab/
   decode.rs`), which is why: losses under a third of the decided pairs is wins over two
   thirds. Protocol version 2, for E's new rounds (C1, R5, R6, W1): a group is `inconclusive`
   below a minimum of pairs **picked** (not decided, so a `same` pick never makes it
   inconclusive); else `preferred` when wins reach two thirds of the decided pairs; else
   `not_worse` when losses stay under a third of every pair picked, `same` included; else
   `worse`. With `same` picks it can be reached at any size of round: of 8 pairs picked, 4
   wins, 2 losses and 2 `same` is `not_worse`.
   `decode::not_worse_is_reached_when_same_picks_keep_losses_under_a_third`;
   `decode::a_same_pick_does_not_make_a_version_2_round_inconclusive`;
   `decode::a_version_1_protocol_decodes_as_before`.

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
   these decodes, the task success and preservation of `docs/results/m3-edits.md`, Bria's
   through fal in all.
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
   playground's clicked removal calls it.
4. **Its rung 2, if built** (the orchestrator's, with the owner): on the 20 pictures of
   segment-01, through Bria: task success by mask coverage as in item 1 and by the owner's
   marks of the finished pictures (M2, item 3); preservation exact from each run; visual
   quality from a lab round of 10 pairs against the unshaped fill, picked by the owner and
   decoded under P0's protocol version 2. Rungs 1 and 2 shown, it is unverified and climbs
   with the others in R5. About 0.8 dollars in fills.

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

## K1. The segmenters compared (the orchestrator's; nothing paid)

1. **The measure**: SAM 3, Tiny and the fixed-class segmenter (on the phrases its list holds)
   on the 20 truth masks of `segmentation/segment-01`, on the Mac, by mask coverage and IoU,
   by group, under `eval loop score`'s reading of a truth mask.
2. **`segment`'s rung 2**: this table is mask agreement with references (section 8), so
   `segment` has rungs 1 and 2 and becomes unverified, before R6 credits its upper rungs.
3. **The rule for milestone 8, frozen here**: milestone 8 builds a further segmenter only if
   SAM 3 is under-covered (below 0.95) on at least a quarter of a group's pictures and another
   model in the catalogue is not. Otherwise no further segmenter. W1 records the outcome.
4. **The record**, `docs/results/e-segmenters.md` and its TOML, held by an evidence test.

- [ ] Done when: the record is committed and `segment`'s record names it.

## R5. Rung 3 of the edit recipes (the orchestrator's)

1. **The sample**, 8 cases a recipe, each recipe on inputs of its own kind: `remove`,
   `remove_prompt`, `remove_checked` and the clicked removal (if C1 built it) on 8 pictures of
   `segmentation/segment-01`, two from each group; `repair` (a stroke) and `repair_area` (a
   location) on 8 pictures of `labs/m3-edits/corpus.toml` with the strokes and locations it
   already holds; `seam` on 8 panoramas of `labs/m3-seam/corpus.toml`. In the order of the
   ladder: `remove_checked`, `seam`, `repair_area`, `repair`, `remove_prompt`, `remove`.
2. **The gates, frozen before the run** (M2): one `edit-gate/1` file a recipe,
   `gates/e-edit-rung3-<recipe>.toml`: task success is the share of finished edits that `check`
   under the default judge passes, at least 0.75, with its family-resampled 95% interval,
   corrected across the recipes of the round, above 0.4 at its lower bound; at least 4
   families (segment-01's four groups count as families of their pictures' scenes, the
   m3-edits corpus's five scenes, the m3-seam corpus's panoramas each its own); preservation
   outside the mask exact on every run; blinded quality from a lab round of this round's 8
   runs against the source picture, picked by the owner and decoded under P0's protocol
   version 2, `preferred` or `not_worse`; operational errors under 10%; cost and latency as M2
   states. Too little data is inconclusive.
3. **The run**, in the loop store, SAM 3 on the Mac and Bria through fal; the cap of one call
   is `remove_checked`'s three-attempt bound under the default judge, plus a cent. Then the
   pages, written to owner storage; the owner picks about 56 pairs; `eval edits gate`
   decides.
4. **What it decides.** Passed: the recipe goes on to R6. Not passed: it ends E
   experimental, recorded.

- [ ] Done when: `docs/results/e-edit-rung3.md` and the receipts are committed, held by an
  evidence test.

## R6. Rung 4 of the edit recipes on fresh cases (the orchestrator's, with the owner)

1. **The fresh corpus**, in owner storage, never committed, none of it used on rungs 2 or 3:
   - 10 of the owner's own photos, each with the phrase of what to remove, where one pass may
     leave something behind (the carry-over row on the owner's photos), for the removals;
   - 10 of the owner's photos with a scratch or a tear, each with a stroke and an area the
     owner draws in the playground and exports as a location (`store export`), for `repair`
     and `repair_area`;
   - 10 panoramas of the owner's, not in `labs/m3-seam/`, for `seam`.
   Each photo is its own family: they are of different scenes, which the owner is asked for.
2. **The gates, frozen before the run** (M2), one a recipe that passed R5: task success from
   the owner's blinded marks of each finished picture (the named thing or defect gone; nothing
   else damaged), at least 0.75 with its family-resampled 95% interval, corrected across the
   round's recipes, above 0.4 at its lower bound, at least 8 families; preservation outside
   the mask exact; blinded quality from a lab round against the source picture as reference
   arm, `preferred` or `not_worse` by P0's protocol version 2, at least 8 pairs picked; cost
   and latency as M2 states. Section 8: the marks and picks are not squint's.
3. **The run** in the loop store; `eval loop sheet` for any recipe (M2, item 3) and `whizbang
   lab pairs` write the sheet and the pages to owner storage; the owner marks and picks;
   `eval edits gate` decides.
4. **What it decides.** Every measure passed: the recipe is admitted. Otherwise experimental.
   **`segment` and `fill`** are credited with rungs 3 and 4 from `remove_prompt`'s
   receipts (section 8, "Earlier evidence counts"): every mask there is SAM 3's and every fill
   Bria's through fal, the same backends `segment` and `fill` default to. If `remove_prompt`
   is not admitted, each ends E experimental, since unverified is not an end state of row E.
   `segment`'s rung 2 is K1's, which runs before this round.
5. **The record**, `docs/results/e-edit-rung4.md`, aggregates only.

- [ ] Done when: the receipts are recorded. If the owner says the photos or panoramas are not
  wanted, the recipes that needed them end E experimental, said in the record and the row.

## W1. What milestone 8 builds (medium; the orchestrator's measure)

1. **A further segmenter**, from K1's table and its rule, frozen in K1.
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
| Which tools a gate admitted | M1, item 7 |
| The 2026-11 study's one repetition | R4, item 7 (tier 2), or withdrawn by N1 |
| The study's results by kind of failure | The owner's answer ("I need", item 5); if yes, `scripts/baseline/tables.py` adds the table in N1 |
| A clicked removal is filled as `segment` found its mask | C1 |
| The Google rows of prices | A0, item 1, at the first Google round on or after each date |
| A resumed matrix never runs a failed cell again | M1, item 8 |
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
| The judge round's figures copied by hand | M1, item 9 |
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
- **Rollback checkpoints.** The tag `m7` (exists). `mE-tools`, when A0, M1, M2, B1, T1, T2 and F1
  have landed: the code, no default changed, no round run. `mE-squint`, on R4's records'
  merge. `mE`, on N1's merge. A pushed tag is never moved.
- **If a task shows a decision here is wrong,** this file is changed first, in its own pull
  request; a change of scope gets review rounds first. A frozen rule is never changed after
  its round started; a new rule is a new round.

| Tasks | To undo |
|---|---|
| A0 | Revert the merge: `describe` shows nine marks again |
| M1 | Revert the merges: gate files of version 3 and matrices with lists are refused again |
| M2 | Revert the merge: edit gates are refused |
| B1 | Revert the merge: no set is changed by it |
| T1, T2 | Revert the merges: the tool files and the entry go; their records stay as history |
| F1 | Revert the merge: v3 is a setting no default uses before R4 |
| R4 | If it changed a default, revert its records' merge or set the setting per call; the receipts stay |
| P0, L1, L2, C1, P1 | Revert the merge; each changes no default but the loop's template (L2), whose old version is still read |
| Rounds (R2, R3, F2, V1, R5, R6, W1) | Records and receipts |

## Spend

All on approved recipients, run by the orchestrator on the owner's Mac. The rates are
`docs/results/e-judges.toml`'s cost per decision under arm C, on source fidelity for the
rounds on its sets (Haiku 0.00357, Terra 0.01106, Flash 0.01842, Sol 0.02238, Argon 0.02976)
and over all sets for F2's dev round (Haiku 0.00352, Flash 0.01659). Argon's price doubles
from 2026-11-30, so 0.0595 in every round of E; Flash's doubles from 2026-12-31. A tool's arm
is taken at 1.5 times its control. Bria through fal at 0.04 a fill. These are estimates, not
limits: the limits are each store's allowance, each call's cap and the builder's `--cap-usd`.

**What E has left: 63.88 dollars** of the milestone's 100, since the first slice settled 36.12.

**Tier 1, within what is left.**

| Task | What is paid | Arithmetic | Estimate (dollars) |
|---|---|---|---|
| A0, M1, M2, B1, T1, T2, F1, P0, L1, L2, C1, P1 | Nothing: fakes in every test; SAM 3 and the fixed-class segmenter on the Mac | | 0 |
| R2, V1, W1 | Nothing: local models and the owner's picks | | 0 |
| D1 | About 70 pictures generated for 22 families and the seasonal pair, under `--cap-usd 8` | | 6 |
| R3 | 143 items: Haiku under C and up to seven treatments | 143 × 0.00357 + 7 × 143 × 0.00357 × 1.5 | 5.9 |
| F2 | 110 dev items under v1 and v3, and 22 of the new versions, Haiku and Flash | (2 × 110 + 22) × (0.00352 + 0.01659) | 4.9 |
| R4 | 110 items: C under five judges; the format under five; two tools under two; the plain twin under two | 12.6 + 12.6 + 4.8 + 1.6 | 31.7 |
| C1 | If built: 20 fills for its rung 2 | 20 × 0.04 | 0.8 |
| R5 | 56 runs, each a fill and a check by Argon; `remove_checked`'s 8 up to two more attempts | 72 × (0.04 + 0.0595) | 7.2 |
| R6 | 30 fresh cases: 10 under four removals, 10 under two repairs, 10 panoramas; two checks a loop | 70 × 0.04 + 20 × 0.0595 | 4.0 |
| | **Expected** | | **about 60** |

If Flash's price has doubled before a round, the round changes so that tier 1 still fits: F2
runs Haiku alone (4.0 less than its estimate), and R4's Flash cells cost 4.0 more, so R4's
second tool's arms move to tier 2 (2.4 less), per-subject comparison keeping its slot: about
58 if both rounds run after the date, about 62 if only R4 does.

**Cap: 63 dollars settled**, counted across every store and the builder's total. A round that
would take the milestone past it does not start; a round that passes twice its estimate stops
and is said. There is no allowance for a round run again after a fix: that is tier 2.

**Tier 2, only on the owner's yes** ("I need", item 2), about 40 more:

| What | Estimate (dollars) |
|---|---|
| The study's comparison repeated: `regions` on Flash, two repetitions of C and of `C+regions` on `confirm-v1` (one of C is tier 1's) | 8 (16 after Flash's price doubles) |
| Flash beside Haiku and Terra in each tool's gate on fresh data | 3 a tool |
| A third tool's arms on fresh data | 2.4 |
| One round run again after a fix | 10 |
| A margin for prices and retries of the loop | 10 |

**E needs more than it has left.** Tier 1 spends nearly all of the 63.88 and leaves nothing
for a round run again, and it confirms at most two tools on fresh data. Tier 2 lets the
study's one claim be tested rather than withdrawn, reads tools under a third judge and allows
a rerun. So this plan asks the owner to raise E's ceiling to 140 dollars in all ("I need",
item 2), and plans nothing past 100 without that yes.

**Stores.** R3 and F2 run in the evaluation store (about 40.94 left): their forecasts, at the
study's rates doubled, are under 15 together. R4 needs a store of its own: its forecast, at
the study's rates doubled, is about 55 in tier 1, more than any store has left, so the
confirmation store's first allowance, 75 dollars, is a reset and the owner's ("I need", item
1). The allowance is a guard; what counts is settled. R5 and R6 run in the loop store (about
17.71 left).

## Dependencies

```
A0 ──────────────────────────────────┐
M1 ─────────────────────┐            │
T1 ──► T2 ──► R2 ──► R3 ┼──► R4 ─────┼──► W1 ──► N1
B1 ──► D1 ──────────────┤            │
F1 ─────────► F2 ───────┘            │
M2 ──────────────────┐               │
P0 ──► V1 ───────────┤               │
L1 ──► L2 ───────────┼──► R5 ──► R6 ─┘
T2 ──► K1 ─────────────────────► R6
C1 ──────────────────┤
P1 ──────────────────┘
```

- T2 after T1: its methods extend T1's tools.
- R2 after T2: both methods of each tool are scored together.
- R3 after M1 and R2: several gates on one matrix, the second look under C, and only tools
  that passed rung 2.
- F2 after F1, M1 (`submission/2`) and D1's dev versions.
- R4 after D1's fresh twins, M1, R3 and F2: only tools that passed rung 3, the format F2 sent,
  and the owner's yes to use the fresh set.
- R5 after M2 (its gates), V1 (rung 2 done first), L2 (the loop as it will stay), C1 and P1
  (the seam's check can look into a panorama).
- K1 after T2 (its model is compared); R6 after K1 (`segment`'s rung 2 before its upper rungs
  are credited).
- W1 after K1, R4 (the fresh set is spent) and R6.

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
- **C1, if built**: the marks of 20 finished pictures and the picks of 10 pairs.
- **R5**: the picks of about 56 pairs of its runs.
- **R6**: in owner storage, 10 of your own photos, each of a different scene with the phrase of
  what to remove; 10 photos with a scratch or a tear, each with a stroke and an area you draw
  in the playground and export; 10 panoramas not used before. Then the marks of R6's sheets
  and the picks of its pages.
- **T2, only if needed**: a yes to a new origin for the segmenter's files, if they are not
  served from `https://huggingface.co`.

## What the owner is told, not asked

- Twenty-four tasks, nine of them the orchestrator's rounds; squint's ladder first, then
  rascal's.
- One fresh set, for source fidelity, kept whole in owner storage until its one round, seen
  by no agent; published after it, as `blind-v3` was.
- The format is redesigned once more (v3); it meets v1 on dev data, and the better of v2 and
  v3 goes to the gate on fresh data.
- The fixed-class segmenter comes back as a catalogue entry for `classes`; SAM 3 stays the
  default.
- `align` loses its mark: it is a control, complete with rungs 1 and 2.
- Details first, `resize` and `tiles` end E experimental.
- At most one change to source fidelity's default recipe comes out of E: the format, or else
  one tool.
- Expected spend about 60 dollars of the 63.88 E has left; cap 63.

## I need

1. **I need your yes to set the confirmation store's allowance to 75 dollars** (yes/no). It is
   a reset, which is yours (section 6); R4's forecast doubles its expected 32. Without it R4
   cannot start.
2. **I need a yes or no: may milestone E spend up to 140 dollars in all, instead of 100?** The
   first slice settled 36.12, tier 1 needs about 60 more, and tier 2 (the study's claim
   tested, a third judge on each tool, a rerun, a margin) about 40. Recommended: yes. With
   no, tier 2 does not run and the study's claim is withdrawn.
3. **I need a yes or no: may a use case of `check` whose gate on fresh data is not run or not
   cleared in E end E unverified, as the default it is today?** That is the four use cases
   other than source fidelity, whose fresh sets move to a later milestone, and source fidelity
   itself if R4 picks no judge. The alternative, experimental, would take `check` away from
   its users. One more fresh set costs about 6 dollars to generate, 25 to 35 to judge and
   about an hour of your review. Recommended: yes.
4. **I need your yes to use the fresh set once, in R4, for the hypotheses its gate files
   state** (yes/no). Section 15 asks first for a held-out set.
5. **I need a yes or no: may the study's results by kind of failure be published?** The kinds
   are your private grouping of `blind-v3`.

## Review rounds

### Review round 1

On draft 1, by the Codex CLI. Verdict: revise. Twelve findings: 1 blocker, 11 majors. Each
was checked against the code; all were right and taken.

1. **Blocker: R6 admitted an edit on preference alone.** Right: section 8 asks task success,
   preservation and blinded quality, with the statistics frozen. Taken: M2 adds `edit-gate/1`
   and `eval edits gate`; R5 and R6 state each measure, its bar, a Wilson bound corrected
   across the round's recipes, and too little data as inconclusive.
2. **Major: the new `not_worse` was still unreachable.** Right (`lab/decode.rs`: `same` and
   `neither` are not decided). Taken: P0's protocol version 2 reads losses over every pair
   picked, `same` included, with a test that it can be reached.
3. **Major: R6's photos cannot test `repair`, `repair_area` or `seam`.** Right (`repair.toml`
   takes a stroke, `repair_area.toml` a location; `seam` a panorama). Taken: R5 runs each
   recipe on inputs of its kind; R6's fresh corpus has photos with a stroke and an area the
   owner draws, and new panoramas.
4. **Major: a matrix runs every cell.** Right (`Matrix::cells`): R4 as drawn would have cost
   about five times its estimate. Taken: M1, item 1, a matrix that runs only the cells it
   names.
5. **Major: no task built `submission/2`, the edits' gates, or the judge rule on gated runs.**
   Right (`gate.rs` gates balanced accuracy; `judges.rs` knows two rules and passes over gated
   runs). Taken: M1, items 3, 5 and 6; M2.
6. **Major: arm C refuses the second look.** Right (`Arm::validate`). Taken: M1, item 2; R3
   names it as a setting of C.
7. **Major: `eval tool` has no `--method`, and `classes` had no number.** Right (`eval.rs`,
   `trying.rs`). Taken: T2, item 6; T1 and T2 state each tool's one number; `classes` is
   scored on the whole frame, so no tool feeds another.
8. **Major: R4's correction left out the judge's rule, which also had no family minimum.**
   Right (`judges.rs`). Taken: `judge-pick/2` (M1, item 6) at the corrected level, with 20
   families; R4 counts every hypothesis, tier 2's too.
9. **Major: defaults changed beyond what was measured.** Right (section 3's recipe; section 8,
   a changed admitted tool returns to unverified). Taken: the format is measured under all
   five judges; at most one change to source fidelity's default is made, and a tool admitted
   beside an admitted format stays one flag away until a gate of the combination.
10. **Major: end states outside row E** (`segment`, `fill`, source fidelity, recipes without
    R6). Right. Taken: `segment` gets rung 2 from W1 and, with `fill`, rungs 3 and 4 credited
    from `remove_prompt`'s receipts, or ends experimental; recipes without fresh cases end
    experimental; "I need", item 3 now covers source fidelity too.
11. **Major: the full wording broke `per_subject`'s subjects.** Right (`per_subject.py`,
    `KEEPS`). Taken: B1, item 5 keeps the "X keeps ..." form, with a test that both twins give
    the same subjects; T1 shares `subjects_of`.
12. **Major: the spend arithmetic.** Right. Taken: each line now shows its arithmetic; F2 runs
    two judges and no v2; R4 confirms at most two tools in tier 1; the total is about 60, about
    62 if Flash's price has doubled, under the cap of 63, and the plan says plainly that E
    needs the owner's yes for more.

### Review round 2

On draft 2, by the Codex CLI. Verdict: revise. Round 1's fixes held but for findings 1, 2, 9,
10 and 12, which this round completed. Eleven findings: 10 majors, 1 minor. Each was checked
against the code; all were right and taken.

1. **Major: the gate and its matrix would name each other's digests.** Right (`gate.rs`'s
   `matrix_identity`; `Matrix::identity` hashes its content). Taken: in version 3 the binding
   runs one way: a gate names its matrix by file only, and the matrix names each gate's
   digest.
2. **Major: the edit gates resampled runs, not families, and stated no cost or latency.**
   Right (section 8, "Gates"). Taken: M2's gate names each case's family, resamples by family,
   states a minimum of families and the limits on cost and latency; R5 and R6 state theirs.
3. **Major: F2 mixed the requirement items into the format's counts.** Right (`judges.rs`
   counts every decision of an arm). Taken: two matrices, the format's on one population,
   the requirements' as a comparison of two set versions; `submission/2` counts only items
   scored under both arms.
4. **Major: defaults still went past what was measured.** Right. Taken: the format changes
   only source fidelity's recipe file, the one use case confirmed; a tool is on by default
   only in the combination its gate measured (gap 10, A0, item 4), so source fidelity's
   default changes by the format alone or not at all.
5. **Major: R4's two slots could leave out per-subject comparison.** Right (row E names its
   rungs 3 and 4). Taken: it keeps a slot in tier 1 whenever it passed R3.
6. **Major: R5's gates had no blinded quality.** Right. Taken: R5 draws pages of its own runs;
   the owner picks; the decode is one of its measures.
7. **Major: `not_worse` was still out of reach on rounds of 8.** Right (`decode.rs`: the
   minimum is of decided pairs). Taken: protocol version 2's minimum is of pairs picked, with
   a test that a `same` pick does not make a round inconclusive and a worked case.
8. **Major: `segment`'s rung 2 came after the round that credits its upper rungs.** Right.
   Taken: K1, a task of its own after T2 and before R6, compares the segmenters and gives
   `segment` its rung 2; W1 reads its outcome.
9. **Major: the clicked removal skipped rung 2.** Right. Taken: C1, item 4, its rung 2 before
   R5: task success, preservation and quality on segment-01.
10. **Major: the late-price case left out F2.** Right. Taken: if Flash's price has doubled
    before F2, F2 runs Haiku alone; the totals of each case are stated, all under the cap.
11. **Minor: A0's spec sentence was narrower than "I need", item 3.** Right. Taken: the
    sentence now matches the question.

