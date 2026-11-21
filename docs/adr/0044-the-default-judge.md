# 0044. The default judge

Status: proposed. Accepted when the judge round's record is in (`docs/results/e-judges.md`).
Date: 2026-11-21
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

Pending: written from the round's TOML (`docs/results/e-judges.toml`, its `[[picks]]`).

1. **The default judge for `check`**: the outcome of `judge-pick/1` over all sets pooled.
2. **The loop's judge**: the outcome over the `source-fidelity` sets, the one use case the
   loop checks with; the same model unless that scope picks another.
3. **Under the format v1**, the default.
4. **What it rests on**: descriptive evidence only (dev sets and an exposed set); it admits
   nothing. No pick keeps `claude-haiku-5-5` and says why.
5. **Chosen again** when a judge is added, a price changes by more than half, the format's
   default changes, or a later slice of E gates on fresh data.

## Consequences

Pending: what the pick changes in the guides (`docs/guides/config.md`'s first judge
example, `docs/guides/loop.md`'s default judge and the call cap three attempts need with it)
and the tool records (each use case's "Alternatives" and "Speed and cost";
`remove_checked`'s judge). A revert of the pull request names `claude-haiku-5-5` again.

## Alternatives

Pending: each judge the rule did not pick, and the clause it failed.
