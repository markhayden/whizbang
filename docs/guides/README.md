# Guides

One page per door and per "how to add" task (`SPEC.md` section 12). None is written yet: a
guide ships with the milestone that makes its subject work, and is listed here until then.
Milestones are those of `SPEC.md` section 16.

## Doors

| Guide | Covers | Milestone |
|---|---|---|
| The command line | `whizbang` for a person and for scripts | Checkpoint 1A |
| The MCP server over stdio | `whizbang serve` for a local agent | 2 |
| The playground | `whizbang play`: trying tools in a browser | 5 |
| The MCP server over HTTP, and remote hosting | `whizbang serve --http` as the engine of a hosted app; a model on a GPU host | 6 |

## How to add

| Guide | Covers | Milestone |
|---|---|---|
| Adding a model | An adapter, its catalogue entry and its conformance fixtures | Checkpoint 1D |
| Adding a tool | Writing an experimental tool, trying and scoring it, and taking it up the ladder of `SPEC.md` section 8. Says that an experimental tool and a user's adapter are trusted code | 2 |
| Running an evaluation | Freezing a gate, running it, writing up the result | 2 |
| Writing a recipe | A named, typed sequence of steps over primitives and other recipes, including a user's own recipe file | 3 |
| Adding a top-level tool | A tool beside rascal and squint | When Pixel starts |

## Rules for a guide

- An example is marked with the test tier that runs it. In the fast tier it uses offline
  fixtures; a documentation check never spends.
- A guide does not repeat the reference. What a build can do is in `docs/reference/`, which
  is generated.
- The procedures agents follow (starting a task, porting a module, reviewing and merging,
  cutting a release) are skills under `.claude/skills/`, not guides.
