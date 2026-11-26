# 0042. Submission v2

Status: accepted.
Date: 2026-11-21
Spec: section 3 ("What every run records"), section 7 (the use cases), section 8 ("Marks"),
section 16 (row E), section 17 (the submission "ported as is, then redesigned through the
ladder"); plan `docs/plans/e1-judge-format-loop.md`, task F1
Amended: 2026-11-21, with task F2 of the same plan: the format round did not adopt v2, by
clause (c) of `submission/1`; v1 stays the default. Added: "The format round".
Amended: 2026-11-25, with plan `docs/plans/mE-evidence-and-edit-quality.md`, task F1: a third
value, `v3`. Added: "Amended: submission v3".

## Context

`check` under a use case of the table asks its judge for one structured answer, the tool
`submit_verdict`, ported from squint byte for byte: a list of items, each naming its checklist
id, and a list of findings. A submission that breaks a rule is answered with the first
problem found and asked for once more; a second rejection ends the run as
`invalid_submission`. Small models fail this form: in milestone 2's evaluation Haiku needed
51 repairs in 220 decisions and ended 10 as invalid submissions
(`docs/results/m2c-evaluation.md`). Task T1 of the same plan records each rejection by class
and each run's cause of ending (`rejections.json`), so the effect of a new form can be
counted.

## Decision

1. **A setting, `submission`, `v1` or `v2`,** on `check` (default `v1`) and on the five
   use-case recipes (`submission = "v1"` in the judge's step, set with
   `--set judge.submission=v2`), on `whizbang squint check` (`--submission`), and in a matrix
   arm (`C;submission=v2`, or `submission = "v2"` in a table). It is in the decision cache's
   key, since every argument of `check` is, and in each run's recorded arguments. An arm
   names it in its identity only when it gives it, so every matrix written before keeps its
   identity. A matrix resumed on a binary whose default differs from the value a completed or
   failed cell of an arm that names none recorded is refused before anything runs
   (`whizbang::evaluation::submission_resumable`). `check` is 1.8.0.
2. **v1 is unchanged:** its schema, description, messages and one repair, byte for byte
   (`submission::the_judge_sees_the_same_messages_as_before`), and the baseline replay runs on
   it.
3. **v2 keys the items by a key per checklist item.** The tool (`submit_verdict_v2.json`,
   written per job by `use_case::submit_tool_v2`) gives `items` as an object with one
   required key for each checklist item, `additionalProperties` false, each value v1's item
   answer without `id`, its description naming the item's id; `findings`, `opinion` and the
   definitions are v1's. A key is not the id itself: checklist ids hold `:` and `@`, and
   Anthropic holds a property's name to `^[a-zA-Z0-9_.-]{1,64}$` (Google may hold it to
   something alike), so each key is the id with every other character made `_` and cut to
   64 (`chk:ob:SF1@candidate` is `chk_ob_SF1_candidate`); a key that would repeat an earlier
   one, or come out empty, is `item_` and the item's place instead (`use_case::item_keys`).
   The keys are one to one with the items, and the same for the same checklist. Reading a
   submission maps each key back to its id; the id itself is read as its key too, and
   `id_case` applies to keys and ids alike; any other key stays unknown. The adapters send
   the tool as they send v1: as written to Anthropic, not strict to OpenAI, its definitions
   written in place for Google, and a test of each holds every property name to the
   pattern. Whether each provider takes the form is shown only by a live request: the
   orchestrator sends one per provider before F2.
4. **v2 reads near-misses that have one reading, and records each** in `rejections.json`
   under `recoveries`, `{turn, kind}`: `list_as_text` (a list, or the keyed object, given as a
   string that holds it), `array_form` (v1's list of answers, each with its id), `enum_word`
   (a disposition, confidence or location kind differing in case, surrounding space, or a
   space or hyphen for an underscore; `can't tell` and `cannot_tell` for `cant_tell`),
   `id_case` (a key or a checklist id differing only in case or surrounding space, exactly
   one item matching), `identical_duplicate` (a second answer of an item the same in disposition,
   confidence, location and `allowed_by`, read as the first), `evidence_dropped` (an id of a
   call that did not complete, or evidence that is null) and `wrapper` (the submission inside
   an object whose only key is `submission` or `verdict`, once). Recoveries are recorded for a
   submission that was accepted. A differing duplicate, an id that matches none or more than
   one, and a violation without a location are never recovered. An answer under a key that
   is not an object (a string, null) is refused as `bad_field` in v1's words for that item,
   and counts as answered, so it is not also missing.
   **A key written twice.** A JSON object cannot hold a key twice, and a parser keeps the
   last, so a second answer under one key is gone once the arguments are parsed. Where the
   text is at hand it is read for keys written twice (`capability::repeated_keys`): the
   OpenAI adapter, which is sent the arguments as text, keeps the pointer of each beside the
   call (`ToolCall::repeated_keys`), and `items` given as a string is read so too; a key
   of an item written twice is refused as `duplicate_item`, in v1's words. **Known limit:**
   Anthropic and Google send the arguments already parsed, so under them a key written
   twice cannot be seen, and the last answer under it is read. A recovered answer is
   accepted under v2; nothing claims it is the verdict a v1 repair would have reached.
5. **v2 names every problem in its one repair.** What is left after the recoveries is held to
   v1's rules unchanged (`decided_of`'s checks, in every-problem mode): the first problem of
   each item and each finding, then the items missing or given twice, each in v1's words,
   naming only ids the judge was given. At most 12 are named, then `and N more`. The
   rejection is recorded by its first problem's class, with the classes of the others under
   `also`. Still one repair, and the repaired submission is a whole submission.
6. **The second look keeps its contract.** v2 applies to the first submission and its repair.
   The first submission is turned into v1's array form before the second look reads it, and
   the second look's turn offers v1's tool, so `tentative_items`, `merged` and the strict rule
   see what they see under v1. The swap of the tool between turns is tested with the fake
   judge only; the live request per provider before F2 covers it on the wire.
7. **v2 is a setting no default uses.** Task F2 runs v1 against v2 on the small judges and
   decides by the rule `submission/1`, frozen in the plan. Adopting v2 means changing
   `check`'s default and the `submission` of the judge's step in all five recipes, which
   state `v1` themselves: the constant alone leaves every recipe, and so every door, on v1.
   This record is amended with the evidence then.
8. **What a rejection names.** A rejection of evidence names an item by its id only when
   that is an id of the checklist, and otherwise "an item", under v1 and v2 alike: nothing
   the judge wrote is said back to it.

## The format round

**The probe.** Before the round, one live check under v2 for each small judge
(`claude-haiku-5-5`, `gemini-3.8-flash`, `gpt-6.1-terra`), from a build at 377a9e7f: all three
providers accepted v2's keyed tool, its keys of the provider-safe form (item 3), and each check
passed. Every judge answered confidently, so none asked for the second look, and the swap of
the tool between turns (item 6) was not exercised on the wire. The round did not exercise it
either: its two arms are arm C, whose second look is off, so no cell ran one. The swap stays
shown by the fake judge only.

**The round** (`matrices/e-submission.toml`; `docs/results/e-submission.md` and `.toml`): v1
against v2 under arm C, on the ten dev sets, under the three small judges, twice: 1,320 cells,
1,286 completed and 34 failed, every decision with its diagnostic, 11.77 dollars settled. The
rule `submission/1` did **not adopt** v2, settled by **clause (c)**:

- (a) held: submission failures fell from 14 under v1 to 4 under v2 (660 decisions an arm),
  all of them `claude-haiku-5-5`'s; first rejections rose from 87 to 101.
- (b) held: no judge had more failures under v2.
- (c) failed for every judge, each moving by more than 2 points (repetition 1, point
  estimates): `claude-haiku-5-5`'s false-reject rate +2.2 points, though its balanced accuracy
  rose 2.3; `gemini-3.8-flash`'s balanced accuracy -5.2 points, its rates unchanged and its
  faults up from 1 to 14; `gpt-6.1-terra`'s balanced accuracy -3.3 points and false-reject rate
  +11.1 points.

**Flash's 14 faults under v2** (one under v1) are all one kind, counted from the store's
records: `judge_fault`, `too_large`, a request counted over its backend's 40,000 input tokens
and refused by the adapter before it was sent (`not_sent`), after 3 to 8 requests of a run;
Haiku's one fault under v2 is the same. None is the provider refusing the keyed schema: no
such request reached the provider, and the rest of Flash's v2 runs completed with that tool.
v2's keyed tool is larger than v1's and the count of input holds it, which likely brought long
conversations to the ceiling sooner; the record does not measure that. Clause (b) counts
submission failures only, so it does not count these runs v2 ended and v1 did not; they count
as errors, and reach the decision through clause (c) as Flash's fall in balanced accuracy.

Of the recoveries, only `evidence_dropped` was used (238 times, all Haiku's). The evidence is
descriptive, on dev sets, and admits nothing either way.

So item 7 stands as written: v2 is a setting no default uses, `check`'s default and the five
recipes stay `v1`, and judge round J1 runs under `C;submission=v1`. v2 stays one setting away
for a later slice, which may narrow it (the bundle's parts were measured together) or gate it
on fresh data.

## Amended: submission v3 (2026-11-25)

Plan mE, task F1.

**What the round counted.** Under v1, `claude-haiku-5-5` ended 14 of its 220 decisions as
invalid submissions, and under v2 4; its rejections, each counted by its class, were under
v1 `evidence` 45, `finding_cites` 32, `missing_item` 10, `bad_field` 3, `location` 3,
`not_a_list` 2, and under v2 `finding_cites` 77, `location` 14, `not_a_list` 5, `bad_field` 1,
with `finding_cites` 6 and `location` 6 more named under `also`
(`docs/results/e-submission.md`, "Rejections, causes and recoveries"). v2's one recovery in
use, `evidence_dropped` (238), took `evidence` out; what was left was `finding_cites`. The
committed record counts rejections, not the rule that ended each failure, so the 14 and the 4
are not broken down by rule there; `whizbang eval judges` now counts each submission failure
by the class of the rejection that ended it, the last of its run (`failures_by_class`, a
row `failure` of the format round's table), so that the next round's record does. The
class `finding_cites` is a finding's `obligation_ids`: none, one not of the list, an
exemption, or no mandatory one, or one that does not judge the finding's image.

**The decision.**

1. **`submission = "v3"`** on `check`, the five recipes (`--set judge.submission=v3`),
   `whizbang squint check --submission v3`, a matrix arm (`C;submission=v3`) and the
   playground's choices, from `check`'s descriptor; the default stays `v1`.
2. **v1's list of items, each with its checklist id**, written as an enum of the checklist's
   ids (no keyed object: Flash's 14 faults under v2 came with its larger tool). A finding on
   an item is written inside its answer, under `findings`, and cites nothing: it bears on
   the item's instances. An item of one instance names it; a finding inside an item of more
   than one (a reference's, an evidence item's) may list which of them it bears on, none
   listed meaning all, from `instances`, an enum of every instance of such an item
   (`use_case::submit_tool_v3`). The schema has one form of item whatever the checklist, so
   it grows by an id or an instance at a time, never by a form: at its largest checklist
   (nine references and 24 evidence items) it stays within 12 KiB with its definitions
   written in place, as the Google adapter writes them
   (`submission::v3s_tool_stays_small_at_its_largest_checklist`). A finding keeps its own
   disposition (`violation`, `allowed_change`, `cant_tell`), confidence, reason,
   `allowed_by`, location and evidence.
3. **Observed findings** stay v1's, in the separate `findings`, their disposition
   `observed` in the schema, citing observe rules.
4. **Read back to v1's form, and decided by v1's rules** (`use_case::read_back`): each
   finding inside an item becomes one of v1's whose `obligation_ids` are its instances'
   obligations, each once, after the item's; then the observed findings. So v3 gives no
   verdict v1 could not, and every check v1 makes of a finding is made of it: its
   disposition, its location against the obligations it cites, an exemption that applies.
   A rejection names a finding inside an item by its item (`chk:ob:SF1@candidate finding
   0`), and the verdict's reason does too.
5. **What nesting makes impossible, and what is refused.** A citation that is missing or
   not of the list cannot be written. A finding inside an item that lists an instance the
   item does not hold (another item's, among the enum's), or an item's findings that are
   no list, is refused under a new class, `item_findings`, counted in `rejections/2` (whose
   contract is unchanged; a reader of an earlier build counts the name as unknown) and on
   its own row of `runs stats --rejections` (`class item_findings`). An exemption has no item: an answer under an
   id that names one is refused as unknown, and its findings as v1 refuses a finding that
   cites an exemption, in v1's words; so is a listed instance that names an exemption. No
   new rule between an item's answer and its findings is added.
6. **v2's one repair naming every problem, and its recoveries, are kept,** applied to the
   findings inside items as to the others; `array_form` is not, since a list is v3's own
   form, and a keyed object is not read.
7. **The second look** keeps its contract, as under v2: v1's tool, on the first submission
   read back.
8. **v3 is a setting no default uses.** Its dev round (plan mE, task F2) decides whether v3
   or v2 goes to the gate on fresh data.

**Why it is shaped so.** The counts say small models fail the citation, not the list: v1's
list form drew no fault, and v2's keyed object did; `finding_cites` was most of what v2 left.
Nesting moves the citation from the judge to the checklist, and the read-back keeps the
verdict v1's.

## Consequences

- **Amended with v3:** `check` is 1.9.0, for the value `v3`, the evidence items a tool file's
  regions add and the hints given it, so every key of the decision cache made by 1.8.0
  misses.

- A run's arguments now hold `submission`, so every key of the decision cache made by 1.7.0
  misses, as any version change of `check` makes it do.
- `rejections.json` gains `submission` and `recoveries` beside T1's fields, and an `also` on
  a rejection of v2 that named more than one problem; its contract is `rejections/2`.
- The bundle is measured together: all problems at once, the recoveries and the keyed schema.
  The recoveries are counted by kind, so the record shows which were used; separating the
  schema's effect is a later slice's.
- A provider may still refuse the keyed form; the keys hold only what Anthropic's pattern
  allows, and each adapter has a test of the form it sends.
- Reverting is a revert of the task's merge: no default uses v2 before F2.

## Alternatives

- **A partial repair** (asking again for the failing items only): it cannot reuse `merged`,
  and a repaired submission would no longer be whole. Dropped in the plan's first review.
- **Building v2 only after the judge round reads the classes:** puts the format behind the
  judge and the slice in series for a smaller change; the bundle is stated as a risk instead.
- **Recovering everything plausible** (an ambiguous id, a differing duplicate): a reading that
  is one of two is a guess at the verdict; refused.
