# The baseline's live rerun

Plan 2C, task E2, item 8, built by plan 3, task G1: `whizbang eval baseline --live`, run by
the orchestrator on 2026-11-19 at main `3b6824c`, ran arm C of `claude-haiku-5-5` on
`source-fidelity/blind-v3` once, fresh, and compared each decision with the baseline
package's recorded run. It settled 0.514436 US dollars. Descriptive: it decides nothing.

Two numbers, as the plan asks:

- **Agreement with the recorded run:** 111 of 143 decisions compared (77.6%).
- **The recorded run's self-disagreement:** 29 of 143 (20.3%): how often the recorded run's
  later repetition differed from its first, the most two runs of the same model could be
  expected to agree.

Only aggregates: no item, no verdict of an item. The fields are the receipt the command wrote,
with the commit it ran at.

```toml
commit = "3b6824c59178db9f27284b9ae35bf6c4bbcd64bd"
package_digest = "sha256:d2edd3bb789370d2e99e85a212c07f31a7b1e00e76d98ce38b7170ea9db7e726"
set = "source-fidelity/blind-v3"
model = "claude-haiku-5-5"
arm = "C"
live_decisions = 143
live_failed = 2
compared = 143
agreed = 111
recorded_later = 143
recorded_differing = 29
settled_usd = "0.514436"
```
