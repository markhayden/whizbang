# Whizbang

One toolkit for working on images with models: finding and editing things in an image
(`rascal`), judging an image against rules (`squint`), and a playground for both.

**State:** little a person would use yet, and nothing that runs a model. Checkpoint 1A,
the contracts and the store, is complete: [docs/plans/m1a-note.md](docs/plans/m1a-note.md).
At its tag, `m1a`, one command runs an action: `whizbang rascal grow` grows a mask that is
in a store and commits the result. `check`, squint's judge loop, is described and has no
command, and cannot run on this build, which holds no judge. `whizbang runs list|show`
reads what a run recorded, `whizbang store list|delete|prune|import` looks after a store,
and `whizbang describe` says what the build holds. Checkpoint 1B, policy and controls, is
next. The design is in [SPEC.md](SPEC.md); the plans are in [docs/plans/](docs/plans/). It replaces four earlier projects, which stay available as
reference; their final commits are pinned in [docs/sources.toml](docs/sources.toml).

What a build can do is listed in [docs/reference/actions.md](docs/reference/actions.md), which
is generated from `whizbang describe --json --pretty` and never edited by hand.

## Working on it

```
just setup    # once, on a fresh clone: git hooks and the pinned tools
just check    # before a commit: format check, lint, repo checks, fast tests
just scan     # before a push, after `just check`: the secrets scan and the clean-room scan
just docs     # regenerate docs/reference/ after changing what `whizbang describe` reports
```

`just scan` needs a private word list that only the owner has, so the pre-push hook refuses
a push from a machine without it. The rules for working here, for people and agents alike,
are in [AGENTS.md](AGENTS.md).

## Where things are

| Path | What |
|---|---|
| [SPEC.md](SPEC.md) | How whizbang works. The first authority when documents disagree |
| [AGENTS.md](AGENTS.md) | Rules for any agent: commands, roles, boundaries, gotchas |
| [docs/plans/](docs/plans/) | One short plan per milestone |
| [docs/carry-over.md](docs/carry-over.md) | The only backlog |
| [docs/baseline/](docs/baseline/README.md) | What the private baseline package holds, and its public manifest |
| [docs/results/](docs/results/2026-11-tool-study.md) | Evaluation results, aggregates only; first, the 2026-11 tool study |
| [docs/adr/](docs/adr/README.md) | Decisions that are costly to reverse |
| [docs/guides/](docs/guides/README.md) | One page per door and per "how to add" task, as each ships |
| [docs/reference/](docs/reference/) | Generated from the build; never edited by hand |
| `crates/` | The Rust workspace |
| `scripts/` | The scans, the checks behind `just`, the git hooks and their tests |

Licence: Apache-2.0.
