# Whizbang

One toolkit for working on images with models: finding and editing things in an image
(`rascal`), judging an image against rules (`squint`), and a playground for both.

**State:** milestones 1 to 6 are done (their notes are in [docs/plans/](docs/plans/)): the
last of them, the engine API with SAM 3 on a GPU host, is available as `whizbang serve
--http`. Milestones 7, E and 8 come next, in that order. Whizbang runs on the command line,
as a stdio server for agents, as an engine for a hosted app, and in a browser:

- `whizbang rascal …` finds what a phrase names, removes it and repairs a spot, with SAM 3
  on the Mac and a filler in the cloud; `whizbang rascal remove-checked` removes and has
  squint check the result, trying again when it fails.
- `whizbang squint check` has a judge look at an edit through evidence tools and conclude
  pass, fail or unsure, under five use cases.
- `whizbang serve` offers both to an agent over stdio ([docs/guides/serve.md](docs/guides/serve.md)),
  and `whizbang serve --http` to a hosted app as its engine, the engine API, with SAM 3 on a
  GPU host of your own ([docs/guides/engine-api.md](docs/guides/engine-api.md),
  [docs/guides/gpu-host.md](docs/guides/gpu-host.md)).
- `whizbang play` opens the playground in Chrome: run a task from a picture and words,
  watch it, read its report, and find every run again in History
  ([docs/guides/playground.md](docs/guides/playground.md)).
- `whizbang runs list|show|stats|export` reads what every run recorded, `whizbang store …`
  looks after a store, `whizbang spend …` sets what it may spend, and `whizbang models …`
  installs the local models.
- `whizbang runs bundle` carries a run, or a chain of runs, out of a store as one file, and
  `whizbang bundle import|show|list|delete` brings it into another store, reads it, and
  lists and removes those a store holds
  ([docs/guides/bundles-and-replay.md](docs/guides/bundles-and-replay.md)).

The design is in [SPEC.md](SPEC.md); the plans are in [docs/plans/](docs/plans/). It replaces four earlier projects, which stay available as
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
