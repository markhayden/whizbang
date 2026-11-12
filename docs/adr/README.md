# Decision records

An ADR records one decision that is costly to reverse, so that a later reader knows what was
chosen, why, and what was turned down. The series starts at 0001. The old repos' ADRs are
reference only and are not continued here.

## When one is needed

- **A choice that is costly to reverse:** a file or wire format, a protocol, an identity
  scheme, a runtime, a dependency the design leans on.
- **A choice that contradicts or changes `SPEC.md`.** The spec is edited in the same pull
  request, so the two never disagree.
- **A detail the spec says is "decided at" a checkpoint.** What the spec already says binds
  the decision, and the ADR is merged before code depends on it.

Nothing else needs one. A choice that a later commit can undo cheaply belongs in the commit
message; work put off belongs on `docs/carry-over.md`.

## Rules

- One file per decision: `docs/adr/NNNN-short-title.md`, numbered in order from `0001`.
- When documents disagree, `SPEC.md` comes first, then the milestone plan, then an ADR.
- An accepted ADR is not rewritten. A new ADR supersedes it, and each names the other in its
  status line.
- An ADR goes through the same pull request, review and scans as any change. It names no
  agent or model, and holds nothing private.

## Template

```markdown
# NNNN. Title, as the decision in a few words

Status: proposed | accepted | superseded by NNNN
Date: YYYY-MM-DD
Spec: the sections of SPEC.md this touches

## Context

What forces the choice: the constraint in the spec, the measurement, the problem. Facts
only, with where each can be checked.

## Decision

What is chosen, stated so that code and tests can be held to it.

## Consequences

What becomes easier, what becomes harder, what it costs to reverse, and what must change
with it (spec sections, tests, the carry-over list).

## Alternatives

Each option that was turned down, and the reason in a line or two.
```
