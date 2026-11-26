# 0044. The default judge

Status: accepted.
Date: 2026-11-22
Spec: section 8 ("Marks": an unverified tool may be a default while nothing admitted
replaces it; "Evaluation data": `blind-v3` is exposed and never admits a tool), section 6 (a
limit is enforced, an estimate is advice), section 16 (row E); plan
`docs/plans/e1-judge-format-loop.md`, task J1, item 4. The plan names this record ADR 0041;
that number was taken by "The playground host" before this task, so it is 0044, the next
free one.

## Context

No code picks a judge: the config's `default` judge backend answers `check`, whatever model
it names, and `remove_checked` uses it when `--judge` is not given. The default judge is
therefore the model the config guide's first judge example names
(`docs/guides/config.md`, "Checking an edit with a judge"), which is what a user who copies
it gets. Since milestone 2 that is `claude-haiku-5-5`, chosen for its price and never
compared on whizbang's `check` as it ships. The earlier evidence pointed two ways: in the
2026-11 tool study, on `blind-v3` under the old harness, Haiku's balanced accuracy was 89.0%
against 89.6% to 97.9% for the other four, and it sent 33 of 143 items to review; on the
dev sets in milestone 3's gate, 0.529 against Flash's 0.887.

The format is settled first: the format round kept submission v1 (`submission/1`,
`docs/results/e-submission.md`), so the judge is picked under `C;submission=v1`, the format
`check` answers in by default. The judge round scores the five judges this build prices on
the same 253 items under that one arm, and applies the rule `judge-pick/1` in code
(`whizbang eval judges`), over all sets pooled and over the `source-fidelity` sets alone.

## Decision

From the round's scores (`docs/results/e-judges.toml`; the record, `docs/results/e-judges.md`):

1. **The default judge for `check` is `gemini-4-argon`.** Over all sets pooled,
   `judge-pick/1` picks it: it is the one judge of five within the margin, so the cheapest
   of them. The config guide's first judge example names it, so a user who copies the
   guide gets it.
2. **The loop's judge is `gemini-4-argon` too.** Over the `source-fidelity` sets the rule
   finds no pick: `gemini-4-argon` and `gpt-6.1-sol` are tied for best, and the lower
   bounds of their differences from the best are -0.038 and -0.035, just past -0.03. The
   plan gives the loop check's model "unless the source-fidelity scope picks another"; a no
   pick picks no other judge, so the loop keeps `check`'s. `gemini-4-argon` also failed
   the margin over `source-fidelity`: it is the loop's judge because `check`'s pick carries
   over, not because that scope chose it. This departs from the literal reading of plan
   e1's task S1, item 5, "No pick keeps the current default (`claude-haiku-5-5`)", for the
   loop: that default is ineligible in that scope (decided correctly 0.794, under 0.80),
   and task J1, item 4, gives the loop `check`'s judge unless `source-fidelity` picks
   another, which it did not. So `claude-haiku-5-5` is not taken as the loop's judge.
3. **Under the format v1**, the default the format round kept (`C;submission=v1`).
4. **What it rests on**: descriptive evidence only, the ten dev sets and the exposed
   `blind-v3`, 253 items; it admits nothing, and the gate on fresh data that would admit a
   default judge is owed to a later slice of E.
5. **Chosen again** when a judge is added, a price changes by more than half, the format's
   default changes, or a later slice of E gates on fresh data. The first of these is due:
   `gemini-4-argon`'s introductory price ends on 2026-11-30, and its cost is expected to
   about double then.

## Consequences

- `docs/guides/config.md`'s first judge example is Google's, naming `gemini-4-argon`; the
  others follow. `docs/guides/loop.md` names the loop's judge and the cap three checked
  attempts need with it (5.926080 dollars reserved; a cap of 5.94 covers them), and
  `docs/reference/tools/remove_checked.md` names it. Each use case's record gains the five
  judges on its sets under "Alternatives", and their cost and time under "Speed and cost".
- A check costs more: about 0.0285 dollars a decision settled against Haiku's 0.0035, about
  8 times as much (28.51 against 3.52 dollars per 1,000 checks), and roughly double that
  after Argon's price changes. One check reserves 1.935360 dollars against Haiku's
  0.112768, so a config's `call_cap_usd` must be at least that. One loop attempt under it
  reserves 1.97536 and three 5.92608, so the guide's first example caps a call at 6.00: at
  the old example's 2.00 a loop under it would never try again.
- The pooled pick leans on `blind-v3`, 143 of the 253 items and one source-fidelity set: on
  the dev sets alone `gemini-4-argon`'s balanced accuracy (0.844) is behind
  `gemini-3.8-flash`'s (0.880), with the highest false-reject rate of the five (0.311).
  The record says so.
- The tool records' "Another judge" tables, this ADR's figures and the guides' are copied
  from the TOML by hand; `evidence::the_judges_figures_outside_their_record_are_its_own`
  holds them to it (plan mE, task M1, item 10).
- No code changed: no code picks a judge, and `judges.rs`'s fallback for a no pick still
  names `claude-haiku-5-5`. A revert of the pull request names `claude-haiku-5-5` in the
  guides again.

## Alternatives

- **`claude-haiku-5-5`, kept**: ineligible in both scopes: 7 errors of 253 over all sets
  (2.8%, the limit is 2%) and decided correctly 0.700; over `source-fidelity`, decided
  correctly 0.794. Its 60 unsure verdicts of 253 weigh on its decided correctly.
- **`gpt-6.1-sol`**: eligible, 1.3 points of balanced accuracy behind over all sets and
  cheaper (21.16 dollars per 1,000 checks), but the lower bound of its difference from the
  best, -0.046, is past -0.03.
- **`gpt-6.1-terra`**: eligible and cheaper still (10.49 per 1,000), but 6.3 points behind,
  its bound -0.109 and its decided correctly more than 5 points under the best's.
- **`gemini-3.8-flash`**: eligible over all sets, its bound -0.090 and its decided correctly
  more than 5 points under the best's; over `source-fidelity`, 5 errors of 165 (3.0%).
- **Haiku for the loop, as the no pick names**: not taken, for the reading in item 2 of the
  decision: a no pick is not a pick of another judge, and the judge it names is ineligible
  in the loop's own scope.
